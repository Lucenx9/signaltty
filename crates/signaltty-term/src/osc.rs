//! OSC scanner over the raw PTY byte stream. Extracts:
//! - OSC 9;4 `<state>[;<value>]` → retained progress evidence
//! - OSC 9;9 `<cwd>` (ConEmu) → ignored, never a notification
//! - OSC 9 `<msg>` (iTerm2/ConEmu) → notification
//! - OSC 99 `<meta>;<payload>` (Kitty, lenient) → notification once its
//!   chunks (`i`/`d`/`p` keys, `e=1` base64 text) are complete
//! - OSC 777 `notify;<title>;<body>` (rxvt/Ghostty/WezTerm) → notification
//! - OSC 0/1/2 `<title>` → window/icon title
//! - bare BEL → bell flag
//!
//! Runs independently of the screen parser so attention semantics never
//! depend on VT emulation fidelity.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    Notify {
        title: Option<String>,
        body: String,
        source: &'static str,
    },
    /// Payload after `9;`, kept verbatim for declarative rules.
    Progress(String),
    Title(String),
    Bell,
}

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// Max bytes carried across chunks for a split sequence.
const MAX_PENDING: usize = 8192;

pub struct OscScanner {
    pending: Vec<u8>,
    /// Inside an OSC too long to carry: its bytes, terminator included,
    /// are payload, never a bell.
    discarding: bool,
    progress: String,
    /// OSC 99 notification still waiting for its `d=1` chunk.
    kitty: Option<KittyDraft>,
}

/// Bound on an assembled OSC 99 title or body.
const MAX_KITTY_TEXT: usize = 4096;

#[derive(Default)]
struct KittyDraft {
    id: String,
    title: String,
    body: String,
}

impl OscScanner {
    pub fn new() -> OscScanner {
        OscScanner {
            pending: Vec::new(),
            discarding: false,
            progress: String::new(),
            kitty: None,
        }
    }

    /// Latest valid OSC 9;4 payload for this scanner/PTY, empty until reported.
    /// A new process gets a new scanner; malformed updates leave it unchanged.
    pub fn progress(&self) -> &str {
        &self.progress
    }

    /// Scan a chunk; returns events found. Handles sequences split
    /// across chunk boundaries via an internal carry buffer.
    pub fn push(&mut self, data: &[u8]) -> Vec<OscEvent> {
        let mut buf: Vec<u8> = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut events = Vec::new();
        let mut i = 0;
        if self.discarding {
            match find_osc_end(&buf, 0) {
                Ok((end, term_len)) => i = end + term_len,
                Err(Some(next)) => i = next,
                Err(None) => {
                    self.pending = trailing_esc(&buf);
                    return Vec::new();
                }
            }
            self.discarding = false;
        }
        while i < buf.len() {
            if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b']' {
                match find_osc_end(&buf, i + 2) {
                    Ok((end, term_len)) => {
                        let payload = &buf[i + 2..end];
                        let ev = match payload.strip_prefix(b"99;") {
                            Some(pt) => self.kitty(pt),
                            None => parse_osc(payload),
                        };
                        if let Some(ev) = ev {
                            if let OscEvent::Progress(ref progress) = ev {
                                self.progress.clone_from(progress);
                            }
                            events.push(ev);
                        }
                        i = end + term_len;
                    }
                    // Aborted: drop it and rescan from the new OSC.
                    Err(Some(next)) => i = next,
                    Err(None) => break, // incomplete: carry rest
                }
            } else if buf[i] == BEL {
                events.push(OscEvent::Bell);
                i += 1;
            } else if buf[i] == ESC && i + 1 >= buf.len() {
                break; // trailing lone ESC: carry
            } else {
                i += 1;
            }
        }
        let rest = &buf[i.min(buf.len())..];
        self.pending = rest[..rest.len().min(MAX_PENDING)].to_vec();
        if rest.len() > MAX_PENDING {
            // Only an unterminated OSC carries this much: drop it to stay
            // bounded and skip the rest of it.
            self.discarding = true;
            self.pending = trailing_esc(rest);
        }
        events
    }
}

impl OscScanner {
    /// Kitty OSC 99 (`meta;payload`). A notification is chunked: `d=0`
    /// chunks with the same `i` accumulate its title (`p=title`, the
    /// default) and body (`p=body`); it is shown once a chunk without `d=0`
    /// arrives. Lenient: a head that is not metadata is a plain title.
    fn kitty(&mut self, pt: &[u8]) -> Option<OscEvent> {
        let s = String::from_utf8_lossy(pt);
        let (meta, payload) = match s.find(';') {
            Some(idx) if idx == 0 || s[..idx].contains('=') || s[..idx].contains(':') => {
                (&s[..idx], &s[idx + 1..])
            }
            Some(idx) => {
                let title = s[..idx].trim();
                let body = &s[idx + 1..];
                if body.is_empty() {
                    return None;
                }
                return Some(OscEvent::Notify {
                    title: (!title.is_empty()).then(|| title.to_string()),
                    body: body.to_string(),
                    source: "osc99",
                });
            }
            None => ("", &s[..]),
        };
        let (mut id, mut kind, mut done, mut base64) = ("", "title", true, false);
        for pair in meta.split(':') {
            match pair.split_once('=') {
                Some(("i", v)) => id = v,
                Some(("p", v)) => kind = v,
                Some(("d", v)) => done = v != "0",
                Some(("e", v)) => base64 = v == "1",
                _ => {}
            }
        }
        // Queries (`?`, `alive`) and `close` are not notification chunks.
        if !matches!(kind, "title" | "body" | "icon" | "buttons") {
            return None;
        }
        let mut draft = match self.kitty.take() {
            Some(draft) if draft.id == id => draft,
            _ => KittyDraft {
                id: id.to_string(),
                ..KittyDraft::default()
            },
        };
        // `e=1`: the chunk's text is base64 (how `kitten notify` sends it);
        // an undecodable chunk adds nothing rather than showing gibberish.
        let decoded;
        let payload = if base64 && matches!(kind, "title" | "body") {
            decoded = match BASE64.decode(payload.trim()) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => String::new(),
            };
            decoded.as_str()
        } else {
            payload
        };
        match kind {
            "title" => push_capped(&mut draft.title, payload),
            "body" => push_capped(&mut draft.body, payload),
            _ => {} // icon data and buttons are never text to show
        }
        if !done {
            self.kitty = Some(draft);
            return None;
        }
        // A notification without a title uses its body as the title.
        let (title, body) = if draft.body.is_empty() {
            (None, draft.title)
        } else if draft.title.is_empty() {
            (None, draft.body)
        } else {
            (Some(draft.title), draft.body)
        };
        if body.is_empty() {
            return None;
        }
        Some(OscEvent::Notify {
            title,
            body,
            source: "osc99",
        })
    }
}

fn push_capped(text: &mut String, more: &str) {
    for c in more.chars() {
        if text.len() + c.len_utf8() > MAX_KITTY_TEXT {
            break;
        }
        text.push(c);
    }
}

/// A trailing ESC may start the ST that ends a discarded OSC.
fn trailing_esc(buf: &[u8]) -> Vec<u8> {
    if buf.last() == Some(&ESC) {
        vec![ESC]
    } else {
        Vec::new()
    }
}

impl Default for OscScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Find OSC terminator (BEL or ST=`ESC \`) from `from`. Returns
/// (payload_end, terminator_len); `Err(Some(i))` when a new OSC at `i`
/// aborts this one, `Err(None)` when the sequence is incomplete.
fn find_osc_end(buf: &[u8], from: usize) -> Result<(usize, usize), Option<usize>> {
    let mut i = from;
    while i < buf.len() {
        if buf[i] == BEL {
            return Ok((i, 1));
        }
        if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b'\\' {
            return Ok((i, 2));
        }
        // A new OSC aborts the previous one (defensive).
        if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b']' {
            return Err(Some(i));
        }
        i += 1;
    }
    Err(None)
}

fn parse_osc(payload: &[u8]) -> Option<OscEvent> {
    // Split `Ps ; Pt`.
    let semi = payload.iter().position(|&b| b == b';')?;
    let ps = &payload[..semi];
    let pt = &payload[semi + 1..];
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    match ps {
        b"9" if pt == b"4" || pt.starts_with(b"4;") => {
            // Progress is reserved, including malformed messages: never notify.
            parse_progress(pt).map(OscEvent::Progress)
        }
        // ConEmu `9;9;<cwd>`: a prompt's directory report, not a message.
        b"9" if pt.starts_with(b"9;") => None,
        b"9" => {
            let body = text(pt);
            if body.is_empty() {
                return None;
            }
            Some(OscEvent::Notify {
                title: None,
                body,
                source: "osc9",
            })
        }
        b"777" => {
            // `notify;title;body`
            let parts: Vec<&str> = std::str::from_utf8(pt).ok()?.splitn(3, ';').collect();
            if parts.first() != Some(&"notify") || parts.len() < 3 {
                return None;
            }
            Some(OscEvent::Notify {
                title: Some(parts[1].to_string()),
                body: parts[2].to_string(),
                source: "osc777",
            })
        }
        b"0" | b"1" | b"2" => {
            let title = text(pt);
            if title.is_empty() {
                return None;
            }
            Some(OscEvent::Title(title))
        }
        _ => None,
    }
}

/// ConEmu progress states 0..4, optional percentage; -1 is Grok's busy sentinel.
fn parse_progress(payload: &[u8]) -> Option<String> {
    // Bound retained evidence even for a complete OSC delivered in one chunk.
    if payload.len() > 32 {
        return None;
    }
    let text = std::str::from_utf8(payload).ok()?;
    let mut parts = text.split(';');
    if parts.next()? != "4" || !matches!(parts.next()?, "0" | "1" | "2" | "3" | "4") {
        return None;
    }
    if let Some(value) = parts.next() {
        if !value.is_empty()
            && value != "-1"
            && (!value.bytes().all(|b| b.is_ascii_digit()) || value.parse::<u8>().ok()? > 100)
        {
            return None;
        }
    }
    if parts.next().is_some() {
        return None;
    }
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(chunks: &[&[u8]]) -> Vec<OscEvent> {
        let mut s = OscScanner::new();
        let mut out = Vec::new();
        for c in chunks {
            out.extend(s.push(c));
        }
        out
    }

    #[test]
    fn aborted_osc_does_not_starve_later_ones() {
        let ev = scan(&[b"\x1b]9;cut\x1b]0;real-title\x07", b"\x1b]9;next\x07"]);
        assert_eq!(
            ev,
            vec![
                OscEvent::Title("real-title".into()),
                OscEvent::Notify {
                    title: None,
                    body: "next".into(),
                    source: "osc9"
                },
            ]
        );
    }

    #[test]
    fn progress_is_not_a_notification_or_bell_at_any_chunk_boundary() {
        let bytes = b"\x1b]9;4;1;-1\x07\x1b]9;4;0;0\x1b\\";
        for i in 0..=bytes.len() {
            let events = scan(&[&bytes[..i], &bytes[i..]]);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, OscEvent::Notify { .. } | OscEvent::Bell)),
                "boundary {i}: {events:?}"
            );
            assert_eq!(
                events,
                vec![
                    OscEvent::Progress("4;1;-1".into()),
                    OscEvent::Progress("4;0;0".into())
                ]
            );
        }
    }

    #[test]
    fn malformed_progress_is_ignored_and_latest_valid_evidence_is_retained() {
        let mut scanner = OscScanner::new();
        assert_eq!(scanner.progress(), "");
        for payload in ["4;0;0", "4;1;-1", "4;2;100", "4;3", "4;4;", "4;1;25"] {
            let bytes = format!("\x1b]9;{payload}\x07");
            assert_eq!(
                scanner.push(bytes.as_bytes()),
                vec![OscEvent::Progress(payload.into())]
            );
            assert_eq!(scanner.progress(), payload);
        }
        for payload in [
            "4",
            "4;",
            "4;9;1",
            "4;1;101",
            "4;1;-2",
            "4;1;+1",
            "4;1;no",
            "4;1;1;extra",
            "4;1;1\n",
            "4;1;99999999999999999999999999999999999",
        ] {
            let bytes = format!("\x1b]9;{payload}\x07");
            assert!(scanner.push(bytes.as_bytes()).is_empty(), "{payload}");
            assert_eq!(scanner.progress(), "4;1;25");
        }
        assert_eq!(scanner.push(b"\x1b]9;hello\x07\x07").len(), 2);
        assert_eq!(scanner.progress(), "4;1;25");
    }

    #[test]
    fn conemu_cwd_report_is_not_a_notification() {
        // Shell prompts (oh-my-posh `pwd = osc99`, Windows Terminal setups)
        // send OSC 9;9 on every prompt to report the working directory.
        let ev = scan(&[b"\x1b]9;9;/home/me/project\x1b\\", b"\x1b]9;9;/tmp\x07"]);
        assert!(ev.is_empty(), "{ev:?}");
        // A plain OSC 9 message that merely starts with 9 still notifies.
        assert_eq!(
            scan(&[b"\x1b]9;99 tests passed\x07"]),
            vec![OscEvent::Notify {
                title: None,
                body: "99 tests passed".into(),
                source: "osc9",
            }]
        );
    }

    #[test]
    fn osc9_bel_terminated() {
        let ev = scan(&[b"\x1b]9;hello there\x07"]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: None,
                body: "hello there".into(),
                source: "osc9",
            }]
        );
    }

    #[test]
    fn osc777_with_title() {
        let ev = scan(&[b"\x1b]777;notify;Codex;done\x07"]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: Some("Codex".into()),
                body: "done".into(),
                source: "osc777",
            }]
        );
    }

    #[test]
    fn osc99_kitty_lenient() {
        let ev = scan(&[b"\x1b]99;i=1:p=body;task finished\x1b\\"]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: None,
                body: "task finished".into(),
                source: "osc99",
            }]
        );
    }

    #[test]
    fn osc99_kitty_chunks_make_one_notification() {
        // Claude Code's kitty channel: title, body, then an empty `d=1` chunk.
        let ev = scan(&[
            b"\x1b]99;i=3766:d=0:p=title;Claude Code\x1b\\",
            b"\x1b]99;i=3766:p=body;Claude is waiting for your input\x1b\\",
            b"\x1b]99;i=3766:d=1:a=focus;\x1b\\",
        ]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: Some("Claude Code".into()),
                body: "Claude is waiting for your input".into(),
                source: "osc99",
            }]
        );
    }

    #[test]
    fn osc99_kitty_queries_and_icon_data_are_not_messages() {
        let ev = scan(&[
            b"\x1b]99;i=q:p=?;\x1b\\",
            b"\x1b]99;i=a:p=alive;\x1b\\",
            b"\x1b]99;i=n:p=close;\x1b\\",
            b"\x1b]99;i=2:d=0;Build\x1b\\",
            b"\x1b]99;i=2:d=0:p=icon:e=1;iVBORw0KGgo=\x1b\\",
            b"\x1b]99;i=2:p=body;done\x1b\\",
        ]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: Some("Build".into()),
                body: "done".into(),
                source: "osc99",
            }]
        );
        // The documented one-shot form still notifies; a new id drops an
        // unfinished draft instead of mixing it in.
        let ev = scan(&[
            b"\x1b]99;i=3:d=0;stale\x1b\\",
            b"\x1b]99;;Hello world\x1b\\",
        ]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: None,
                body: "Hello world".into(),
                source: "osc99",
            }]
        );
    }

    #[test]
    fn osc99_kitty_base64_payloads_are_decoded() {
        // `kitten notify "Build done" "All 42 tests passed"`: metadata
        // chunk, base64 (`e=1`) title and body chunks, then an empty one.
        let ev = scan(&[
            b"\x1b]99;i=7:d=0:w=-1;\x1b\\",
            b"\x1b]99;i=7:d=0:e=1;QnVpbGQgZG9uZQ==\x1b\\",
            b"\x1b]99;i=7:d=0:e=1:p=body;QWxsIDQyIHRlc3RzIHBhc3NlZA==\x1b\\",
            b"\x1b]99;i=7;\x1b\\",
        ]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: Some("Build done".into()),
                body: "All 42 tests passed".into(),
                source: "osc99",
            }]
        );
        // A payload that is not valid base64 adds no text.
        let ev = scan(&[
            b"\x1b]99;i=8:d=0:e=1;not base64!\x1b\\",
            b"\x1b]99;i=8:e=1:p=body;ZG9uZQ==\x1b\\",
        ]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: None,
                body: "done".into(),
                source: "osc99",
            }]
        );
    }

    #[test]
    fn osc_title_and_bell() {
        let ev = scan(&[b"\x1b]0;my title\x07plain\x07"]);
        assert_eq!(ev, vec![OscEvent::Title("my title".into()), OscEvent::Bell]);
    }

    #[test]
    fn split_across_chunks() {
        let ev = scan(&[b"\x1b]777;notify;Ti", b"tle;Body\x07"]);
        assert_eq!(
            ev,
            vec![OscEvent::Notify {
                title: Some("Title".into()),
                body: "Body".into(),
                source: "osc777",
            }]
        );
    }

    #[test]
    fn oversized_osc_terminator_is_not_a_bell() {
        // OSC 52 clipboard writes routinely exceed the carry buffer.
        let mut stream = b"\x1b]52;c;".to_vec();
        stream.extend(std::iter::repeat_n(b'A', 3 * MAX_PENDING));
        stream.extend_from_slice(b"\x07\x1b]9;after\x07");
        let chunks: Vec<&[u8]> = stream.chunks(MAX_PENDING).collect();
        assert_eq!(
            scan(&chunks),
            vec![OscEvent::Notify {
                title: None,
                body: "after".into(),
                source: "osc9",
            }]
        );
    }

    #[test]
    fn oversized_osc_st_split_across_chunks() {
        let mut stream = b"\x1b]52;c;".to_vec();
        stream.extend(std::iter::repeat_n(b'A', 2 * MAX_PENDING));
        stream.push(ESC);
        let ev = scan(&[&stream, b"\\plain\x07"]);
        assert_eq!(ev, vec![OscEvent::Bell]);
    }

    #[test]
    fn ignores_other_sequences() {
        let ev = scan(&[b"\x1b[31mred\x1b[0m \x1b]8;;http://x\x07link"]);
        assert!(ev.is_empty());
    }
}

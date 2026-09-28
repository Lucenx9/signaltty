//! OSC scanner over the raw PTY byte stream. Extracts:
//! - OSC 9 `<msg>` (iTerm2/ConEmu) → notification
//! - OSC 99 `<meta>;<body>` (Kitty, lenient) → notification
//! - OSC 777 `notify;<title>;<body>` (rxvt/Ghostty/WezTerm) → notification
//! - OSC 0/1/2 `<title>` → window/icon title
//! - bare BEL → bell flag
//!
//! Runs independently of the screen parser so attention semantics never
//! depend on VT emulation fidelity.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    Notify {
        title: Option<String>,
        body: String,
        source: &'static str,
    },
    Title(String),
    Bell,
}

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// Max bytes carried across chunks for a split sequence.
const MAX_PENDING: usize = 8192;

pub struct OscScanner {
    pending: Vec<u8>,
}

impl OscScanner {
    pub fn new() -> OscScanner {
        OscScanner {
            pending: Vec::new(),
        }
    }

    /// Scan a chunk; returns events found. Handles sequences split
    /// across chunk boundaries via an internal carry buffer.
    pub fn push(&mut self, data: &[u8]) -> Vec<OscEvent> {
        let mut buf: Vec<u8> = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut events = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b']' {
                match find_osc_end(&buf, i + 2) {
                    Some((end, term_len)) => {
                        let payload = &buf[i + 2..end];
                        if let Some(ev) = parse_osc(payload) {
                            events.push(ev);
                        }
                        i = end + term_len;
                    }
                    None => break, // incomplete: carry rest
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
            // Pathological run without terminator: drop to stay bounded.
            self.pending.clear();
        }
        events
    }
}

impl Default for OscScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Find OSC terminator (BEL or ST=`ESC \`) from `from`. Returns
/// (payload_end, terminator_len).
fn find_osc_end(buf: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i < buf.len() {
        if buf[i] == BEL {
            return Some((i, 1));
        }
        if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b'\\' {
            return Some((i, 2));
        }
        // A new OSC aborts the previous one (defensive).
        if buf[i] == ESC && i + 1 < buf.len() && buf[i + 1] == b']' {
            return None;
        }
        i += 1;
    }
    None
}

fn parse_osc(payload: &[u8]) -> Option<OscEvent> {
    // Split `Ps ; Pt`.
    let semi = payload.iter().position(|&b| b == b';')?;
    let ps = &payload[..semi];
    let pt = &payload[semi + 1..];
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    match ps {
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
        b"99" => {
            // Kitty form: `meta;body` where meta holds k=v pairs; be
            // lenient: body is everything after the first ';' if the
            // head looks like metadata, else the whole payload text.
            let s = text(pt);
            let (title, body) = match s.find(';') {
                Some(idx) if s[..idx].contains('=') || s[..idx].contains(':') => {
                    (None, s[idx + 1..].to_string())
                }
                Some(idx) => {
                    let t = s[..idx].trim().to_string();
                    (
                        if t.is_empty() { None } else { Some(t) },
                        s[idx + 1..].to_string(),
                    )
                }
                None => (None, s),
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
        let ev = scan(&[b"\x1b]99;i=1:d=0:p=body;task finished\x1b\\"]);
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
    fn ignores_other_sequences() {
        let ev = scan(&[b"\x1b[31mred\x1b[0m \x1b]8;;http://x\x07link"]);
        assert!(ev.is_empty());
    }
}

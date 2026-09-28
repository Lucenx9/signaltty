//! Sanitizers for untrusted text (notification bodies, titles) and
//! ANSI-stripped reads. See docs/10.

/// Strip C0 controls (except \n, \t), C1 controls, and any CSI/OSC
/// escape sequences; cap length. Used for notification title/body and
/// pane titles before storage or fan-out.
pub fn sanitize_notification_text(s: &str, max_chars: usize) -> String {
    let stripped = strip_ansi(s);
    stripped
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Remove ANSI escape sequences (CSI, OSC, charset switches, lone ESC)
/// from text, leaving printable content. Byte-oriented, lossy.
pub fn strip_ansi(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b {
            if i + 1 >= b.len() {
                break;
            }
            match b[i + 1] {
                b'[' => {
                    // CSI: params/intermediates until final byte @-~.
                    i += 2;
                    while i < b.len() && !(0x40..=0x7e).contains(&b[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                b']' => {
                    // OSC: until BEL or ST.
                    i += 2;
                    while i < b.len() {
                        if b[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if b[i] == 0x1b && i + 1 < b.len() && b[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                b'(' | b')' | b'#' => i += 3,
                _ => i += 2,
            }
        } else {
            // Copy one UTF-8 char (or replacement char).
            let rest = &s[i..];
            match rest.chars().next() {
                Some(c) => {
                    out.push(c);
                    i += c.len_utf8();
                }
                None => break,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_and_osc() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1b]0;title\x07text"), "text");
        assert_eq!(strip_ansi("a\x1b]99;x\x1b\\b"), "ab");
    }

    #[test]
    fn sanitize_kills_controls_and_caps() {
        let s = sanitize_notification_text("\x01hi\x1b[1m\x07\nthere\x7f", 100);
        assert_eq!(s, "hi\nthere");
        assert_eq!(sanitize_notification_text("abcdef", 3), "abc");
    }
}

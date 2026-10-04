//! Headless terminal surface: `vt100` screen model plus a bounded
//! raw-line scrollback ring. One per pane, owned by the server.

use std::collections::{HashMap, VecDeque};

use crate::backend::{TermOptions, TerminalBackend};
use crate::sanitize::strip_ansi;

pub const DEFAULT_SCROLLBACK_LINES: usize = 5000;
pub const DEFAULT_SCROLLBACK_BYTES: usize = 1024 * 1024;

struct Surface {
    parser: vt100::Parser,
    cols: u16,
    rows: u16,
    /// Raw output lines (still contain ANSI; stripped at read time).
    lines: VecDeque<String>,
    line_bytes: usize,
    max_lines: usize,
    max_bytes: usize,
    /// Incomplete trailing line fragment.
    fragment: String,
    /// Incomplete UTF-8 sequence cut by a PTY read boundary.
    utf8_carry: Vec<u8>,
}

impl Surface {
    fn new(cols: u16, rows: u16) -> Surface {
        Surface {
            parser: vt100::Parser::new(rows, cols, 0),
            cols,
            rows,
            lines: VecDeque::new(),
            line_bytes: 0,
            max_lines: DEFAULT_SCROLLBACK_LINES,
            max_bytes: DEFAULT_SCROLLBACK_BYTES,
            fragment: String::new(),
            utf8_carry: Vec::new(),
        }
    }

    fn feed(&mut self, data: &[u8]) {
        self.parser.process(data);
        let mut bytes = std::mem::take(&mut self.utf8_carry);
        bytes.extend_from_slice(data);
        let carry = (1..=bytes.len().min(3))
            .find(|&k| {
                std::str::from_utf8(&bytes[bytes.len() - k..])
                    .is_err_and(|e| e.valid_up_to() == 0 && e.error_len().is_none())
            })
            .unwrap_or(0);
        self.utf8_carry = bytes.split_off(bytes.len() - carry);
        let text = String::from_utf8_lossy(&bytes);
        for chunk in text.split_inclusive('\n') {
            if let Some(line) = chunk.strip_suffix('\n') {
                self.fragment.push_str(line);
                let line = std::mem::take(&mut self.fragment);
                self.push_line(line);
            } else {
                self.fragment.push_str(chunk);
            }
        }
        if self.fragment.len() > 64 * 1024 {
            // Bound pathological no-newline output: flush fragment.
            let line = std::mem::take(&mut self.fragment);
            self.push_line(line);
        }
    }

    fn push_line(&mut self, line: String) {
        self.line_bytes += line.len();
        self.lines.push_back(line);
        while self.lines.len() > self.max_lines || self.line_bytes > self.max_bytes {
            if let Some(old) = self.lines.pop_front() {
                self.line_bytes = self.line_bytes.saturating_sub(old.len());
            } else {
                break;
            }
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        if self.cols == cols && self.rows == rows {
            return;
        }
        // No reflow: vt100 truncates/pads rows in place, so the visible
        // screen survives (TUIs redraw on SIGWINCH anyway).
        self.parser.set_size(rows, cols);
        self.cols = cols;
        self.rows = rows;
    }
}

pub struct HeadlessBackend {
    surfaces: HashMap<String, Surface>,
}

impl HeadlessBackend {
    pub fn new() -> HeadlessBackend {
        HeadlessBackend {
            surfaces: HashMap::new(),
        }
    }

    /// Last `n` scrollback lines (oldest→newest), optionally ANSI-stripped.
    pub fn tail(&self, id: &str, n: usize, strip: bool) -> Option<Vec<String>> {
        let s = self.surfaces.get(id)?;
        let mut out: Vec<String> = s.lines.iter().rev().take(n).cloned().collect();
        out.reverse();
        if !s.fragment.is_empty() {
            out.push(s.fragment.clone());
        }
        if strip {
            out = out.iter().map(|l| strip_ansi(l)).collect();
        }
        Some(out)
    }

    pub fn scrollback_len(&self, id: &str) -> usize {
        self.surfaces.get(id).map(|s| s.lines.len()).unwrap_or(0)
    }

    /// Create the surface unless it exists (resume keeps restored tail).
    pub fn ensure_surface(&mut self, id: &str, cols: u16, rows: u16) {
        if let Some(s) = self.surfaces.get_mut(id) {
            s.resize(cols, rows);
        } else {
            self.surfaces
                .insert(id.to_string(), Surface::new(cols, rows));
        }
    }

    /// Recreate a surface pre-filled with persisted tail lines (restore).
    pub fn restore_surface(&mut self, id: &str, cols: u16, rows: u16, lines: Vec<String>) {
        let mut s = Surface::new(cols, rows);
        for line in lines {
            s.push_line(line);
        }
        self.surfaces.insert(id.to_string(), s);
    }
}

impl Default for HeadlessBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalBackend for HeadlessBackend {
    fn create_surface(&mut self, id: &str, cols: u16, rows: u16) {
        self.surfaces
            .insert(id.to_string(), Surface::new(cols, rows));
    }

    fn feed_output(&mut self, id: &str, data: &[u8]) {
        if let Some(s) = self.surfaces.get_mut(id) {
            s.feed(data);
        }
    }

    fn send_input(&mut self, _id: &str, _data: &[u8]) {
        // Server-side backend: input goes straight to the PTY master,
        // not through here. Client backends forward to IPC.
    }

    fn resize(&mut self, id: &str, cols: u16, rows: u16) {
        if let Some(s) = self.surfaces.get_mut(id) {
            s.resize(cols, rows);
        }
    }

    fn snapshot(&self, id: &str) -> String {
        self.surfaces
            .get(id)
            .map(|s| s.parser.screen().contents())
            .unwrap_or_default()
    }

    fn screen_state(&self, id: &str) -> Vec<u8> {
        self.surfaces
            .get(id)
            .map(|s| s.parser.screen().state_formatted())
            .unwrap_or_default()
    }

    fn configure(&mut self, _opts: &TermOptions) {}

    fn destroy(&mut self, id: &str) {
        self.surfaces.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_tail() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"hello\nworld\n");
        assert!(b.snapshot("p").contains("hello"));
        let tail = b.tail("p", 10, false).unwrap();
        assert_eq!(tail, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn tail_keeps_utf8_split_across_reads() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"caf\xc3");
        b.feed_output("p", b"\xa9\n");
        assert_eq!(
            b.tail("p", 1, false).unwrap(),
            vec!["caf\u{e9}".to_string()]
        );
    }

    #[test]
    fn screen_state_replays_colours_and_line_starts() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"\x1b[31mred\x1b[0m\r\nnext\r\n");
        let state = b.screen_state("p");
        // Replaying into a fresh screen reproduces it exactly.
        let mut replay = vt100::Parser::new(24, 80, 0);
        replay.process(&state);
        assert_eq!(replay.screen().contents(), b.snapshot("p"));
        assert_eq!(
            replay.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(1)
        );
        assert_eq!(replay.screen().cell(1, 0).unwrap().contents(), "n");
    }

    #[test]
    fn resize_keeps_the_visible_screen() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"still here\r\n");
        b.resize("p", 120, 40);
        assert!(b.snapshot("p").contains("still here"));
        b.resize("p", 40, 10);
        assert!(b.snapshot("p").contains("still here"));
    }

    #[test]
    fn tail_strips_ansi() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"\x1b[31mred\x1b[0m\n");
        assert_eq!(b.tail("p", 5, true).unwrap(), vec!["red".to_string()]);
    }

    #[test]
    fn scrollback_is_bounded() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for i in 0..(DEFAULT_SCROLLBACK_LINES + 100) {
            b.feed_output("p", format!("line {i}\n").as_bytes());
        }
        assert_eq!(b.ring_len("p"), DEFAULT_SCROLLBACK_LINES);
    }

    // ---- US3 rendered ring (T008): TUI repaint fixtures ----

    fn rendered_text(b: &HeadlessBackend, id: &str) -> String {
        b.rendered(id, 0, 5000).unwrap().text
    }

    #[test]
    fn rendered_cr_overwrite_resolves_to_final_text() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"Status: Starting\rStatus: Ready   \n");
        assert_eq!(rendered_text(&b, "p"), "Status: Ready");
        assert_eq!(
            b.tail("p", 10, true).unwrap(),
            vec!["Status: Ready".to_string()]
        );
    }

    #[test]
    fn rendered_cursor_up_redraw_has_no_duplicates() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"=== hdr ===\nline A\n\x1b[1A\x1b[2Kline B\n");
        let text = rendered_text(&b, "p");
        assert_eq!(text, "=== hdr ===\nline B");
        // tail is rebased on the same ring: identical text.
        let tail = b.tail("p", 200, true).unwrap().join("\n");
        assert_eq!(tail, text);
    }

    #[test]
    fn rendered_erase_line_then_rewrite() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"hello\x1b[2K\x1b[Gbye\n");
        assert_eq!(rendered_text(&b, "p"), "bye");
    }

    #[test]
    fn rendered_spinner_rewritten_50_times_stays_one_line() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for i in 0..50 {
            b.feed_output("p", format!("spin {i}\r").as_bytes());
        }
        let r = b.rendered("p", 0, 5000).unwrap();
        assert_eq!(r.text, "spin 49");
        assert!(!r.dropped);
        assert!(!r.truncated);
    }

    #[test]
    fn rendered_second_read_is_delta_only() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"alpha\n");
        let r1 = b.rendered("p", 0, 200).unwrap();
        assert!(r1.text.contains("alpha"));
        b.feed_output("p", b"beta\n");
        let r2 = b.rendered("p", r1.next_seq, 200).unwrap();
        assert_eq!(r2.text, "beta");
        assert!(!r2.dropped);
        // Unchanged pane: empty text, unchanged head.
        let r3 = b.rendered("p", r2.next_seq, 200).unwrap();
        assert_eq!(r3.text, "");
        assert_eq!(r3.next_seq, r2.next_seq);
        assert_eq!(r3.seq, r2.next_seq);
    }

    #[test]
    fn rendered_bulk_scroll_burst_keeps_order() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        let mut bulk = String::new();
        for i in 0..10 {
            bulk.push_str(&format!("l{i}\n"));
        }
        b.feed_output("p", bulk.as_bytes());
        let text = rendered_text(&b, "p");
        let expect: Vec<String> = (0..10).map(|i| format!("l{i}")).collect();
        assert_eq!(text, expect.join("\n"));
    }

    #[test]
    fn rendered_eviction_reports_dropped() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for chunk in 0..52 {
            let mut bulk = String::new();
            for i in 0..100 {
                bulk.push_str(&format!("line {}\n", chunk * 100 + i));
            }
            b.feed_output("p", bulk.as_bytes());
        }
        assert_eq!(b.ring_len("p"), DEFAULT_SCROLLBACK_LINES);
        let dropped = b.rendered("p", 1, 5000).unwrap();
        assert!(dropped.dropped);
        let full = b.rendered("p", 0, 5000).unwrap();
        assert!(!full.dropped);
        assert!(!full.truncated);
        assert!(full.text.starts_with("line 200\n"));
        assert!(full.text.ends_with("line 5199"));
    }

    #[test]
    fn rendered_truncated_returns_newest_lines() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for i in 0..10 {
            b.feed_output("p", format!("l{i}\n").as_bytes());
        }
        let r = b.rendered("p", 0, 3).unwrap();
        assert!(r.truncated);
        assert_eq!(r.text, "l7\nl8\nl9");
    }

    #[test]
    fn rendered_alt_screen_returns_grid_without_history_pollution() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"main line\n");
        let before = rendered_text(&b, "p");
        assert_eq!(before, "main line");
        // Enter alt screen: main content hidden, alt grid shown.
        b.feed_output("p", b"\x1b[?1049h");
        b.feed_output("p", b"alt content\n");
        let r1 = b.rendered("p", 0, 5000).unwrap();
        assert!(r1.text.contains("alt content"));
        assert!(!r1.text.contains("main line"));
        // Repaint: polling with the cursor yields only the changed row.
        b.feed_output("p", b"\x1b[Halt content v2");
        let r2 = b.rendered("p", r1.next_seq, 5000).unwrap();
        assert!(r2.text.contains("v2"));
        assert!(!r2.dropped);
        // Exit: main grid restored, alt lines never entered history.
        b.feed_output("p", b"\x1b[?1049l");
        let after = rendered_text(&b, "p");
        assert_eq!(after, before);
        assert!(!after.contains("alt content"));
    }

    #[test]
    fn rendered_clear_does_not_flood_history() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"keep\n");
        let len_before = b.ring_len("p");
        b.feed_output("p", b"\x1b[2J\x1b[H");
        assert_eq!(b.ring_len("p"), len_before);
    }

    #[test]
    fn restore_marks_pre_restart_cursors_dropped() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"before\n");
        let old_next = b.rendered("p", 0, 200).unwrap().next_seq;
        assert!(old_next > 0);
        // Simulate restart recovery: content restored, sequence reset.
        b.restore_surface("p", 80, 24, vec!["before".to_string()]);
        let r = b.rendered("p", old_next, 200).unwrap();
        assert!(r.dropped);
        let fresh = b.rendered("p", 0, 200).unwrap();
        assert!(!fresh.dropped);
        assert_eq!(fresh.text, "before");
    }

    #[test]
    fn resize_keeps_scrollback() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"keepme\n");
        b.resize("p", 100, 30);
        assert_eq!(b.tail("p", 5, true).unwrap(), vec!["keepme".to_string()]);
    }
}

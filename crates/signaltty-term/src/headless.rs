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
        }
    }

    fn feed(&mut self, data: &[u8]) {
        self.parser.process(data);
        let text = String::from_utf8_lossy(data);
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
        // vt100 has no reflow: restart the screen model, keep scrollback.
        self.parser = vt100::Parser::new(rows, cols, 0);
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
        assert_eq!(b.scrollback_len("p"), DEFAULT_SCROLLBACK_LINES);
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

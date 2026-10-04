//! Headless terminal surface: `vt100` screen model plus a rendered-line
//! ring with a monotonic content sequence. One per pane, owned by the server.
//!
//! The ring is built from the RENDERED grid, never from raw `\n` splitting:
//! lines that scroll off the top of the vt100 screen move into history, the
//! current screen rows follow, and in-place repaints (CR overwrites,
//! cursor-up redraws, erase-line, alt-screen updates) only refresh the text
//! of the rows they touch. Rewriting one spinner line a thousand times still
//! leaves one ring line behind.
//!
//! Alt-screen note: while a full-screen TUI owns the alternate grid there
//! is no scrollback (engine limit, documented in docs/05, not silent) —
//! reads return the current alt grid, repaints bump the touched rows'
//! sequences so a polling orchestrator sees new content without duplicates,
//! and nothing is appended to history on repaint or on alt enter/exit.

use std::collections::{HashMap, VecDeque};

use crate::backend::{TermOptions, TerminalBackend};
use crate::sanitize::strip_ansi;

/// Rendered-line capacity per pane (history + current screen rows).
pub const DEFAULT_SCROLLBACK_LINES: usize = 5000;

/// One rendered line with its position in the pane's content sequence.
#[derive(Clone, Debug)]
struct RingLine {
    seq: u64,
    text: String,
}

/// Cursor-based read result (`pane.read` rendered mode, contracts/ipc.md).
#[derive(Clone, Debug, Default)]
pub struct RenderedRead {
    pub text: String,
    pub seq: u64,
    pub next_seq: u64,
    pub dropped: bool,
    pub truncated: bool,
}

/// Upper bound on how many rows one feed batch could have scrolled:
/// newlines + explicit scroll controls (VT/FF/NEL/RI, CSI `S`/`T` with
/// counts) + printable-cell rows + one slack. Deliberately generous — it
/// only excludes shifts the batch could not have produced, so repetitive
/// content cannot match a wild `k`.
fn scroll_bound(batch: &[u8], cols: u16) -> usize {
    let mut bound = 1; // slack for control-edge cases
    let mut printable = 0usize;
    let mut i = 0;
    while i < batch.len() {
        let b = batch[i];
        match b {
            b'\n' | b'\x0b' | b'\x0c' | b'\x84' => bound += 1,
            b'\x1b' => {
                let mut j = i + 1;
                if j < batch.len() && batch[j] == b'E' {
                    bound += 1; // NEL
                    i = j;
                } else {
                    if j < batch.len() && batch[j] == b'[' {
                        j += 1;
                        let mut val = 0usize;
                        let mut digits = false;
                        while j < batch.len() && batch[j].is_ascii_digit() {
                            val = val
                                .saturating_mul(10)
                                .saturating_add((batch[j] - b'0') as usize);
                            digits = true;
                            j += 1;
                        }
                        if j < batch.len() && (batch[j] == b'S' || batch[j] == b'T') {
                            bound += if digits { val.max(1) } else { 1 };
                            i = j;
                        }
                    }
                }
            }
            0x20..=0x7e | 0x80..=0xff => printable += 1,
            _ => {}
        }
        i += 1;
    }
    bound + printable / (cols.max(1) as usize)
}

struct Surface {
    parser: vt100::Parser,
    cols: u16,
    rows: u16,
    /// Lines scrolled off the top (oldest→newest, seqs ascending and below
    /// every screen seq).
    history: VecDeque<RingLine>,
    /// Current screen, one entry per grid row (positional order).
    screen: Vec<RingLine>,
    in_alt: bool,
    /// Last assigned sequence number; reported as `next_seq`. Runtime-only:
    /// reset on restart, so pre-restart cursors read as dropped.
    head: u64,
    /// Set by `restore_surface` to the restored head: any nonzero cursor at
    /// or below it is pre-restart and reads as dropped.
    dropped_floor: u64,
    max_lines: usize,
}

impl Surface {
    fn new(cols: u16, rows: u16) -> Surface {
        Surface {
            parser: vt100::Parser::new(rows, cols, 0),
            cols,
            rows,
            history: VecDeque::new(),
            // Lazily sized to the grid on the first feed.
            screen: Vec::new(),
            in_alt: false,
            head: 0,
            dropped_floor: 0,
            max_lines: DEFAULT_SCROLLBACK_LINES,
        }
    }

    fn feed(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        // Reconcile the rendered ring in small batches so a bulk scroll that
        // lands in one PTY read is still captured line by line. Batches cut
        // after every CR/LF: printing a line and scrolling it off are then
        // reconciled as the two separate steps the terminal applied, which
        // keeps strict scroll matching exact (a print+scroll in one step
        // would hide the new line mid-grid and defeat the matcher).
        // Splitting on CR/LF is UTF-8 safe (neither byte appears inside a
        // multibyte sequence) and harmless to the parser, which is a byte
        // state machine that resumes across `process` calls — even mid-OSC.
        // The byte budget (~one visual row) bounds how much new content a
        // single reconcile can carry, so wrap-driven scrolls stay inside
        // the matcher's tail allowance.
        let max_bytes = (self.cols as usize).max(64);
        let mut start = 0usize;
        for (i, &b) in data.iter().enumerate() {
            let cut = b == b'\n' || b == b'\r';
            let bytes = i + 1 - start;
            if cut || bytes >= max_bytes || i + 1 == data.len() {
                let batch = &data[start..i + 1];
                let bound = scroll_bound(batch, self.cols);
                self.parser.process(batch);
                start = i + 1;
                self.reconcile(bound);
            }
        }
    }

    /// Current grid rows as plain text, trailing padding trimmed.
    fn grid_text(&self) -> Vec<String> {
        let width = self.cols.max(1);
        self.parser
            .screen()
            .rows(0, width)
            .map(|r| r.trim_end().to_string())
            .collect()
    }

    fn reconcile(&mut self, bound: usize) {
        let alt = self.parser.screen().alternate_screen();
        let grid = self.grid_text();
        let transitioned = alt != self.in_alt;
        self.in_alt = alt;
        if grid.len() != self.screen.len() {
            // First feed, or a resize: fit lengths, then reconcile in
            // place (never guess a scroll across a size change).
            self.fit_len(&grid);
            self.refresh_in_place(&grid);
            return;
        }
        // On alt enter/exit the grids are unrelated buffers: refresh in
        // place so alt rows never leak into main-screen history.
        if !transitioned && !alt {
            if let Some(k) = self.scroll_by(&grid, bound) {
                self.apply_scroll(k, &grid);
                return;
            }
        }
        self.refresh_in_place(&grid);
    }

    /// Scroll-up `k` (1..=n) consistent with the new grid and the batch's
    /// scroll bound, or None. Batches cut after every CR/LF, so a genuine
    /// scroll advances one line per step.
    ///
    /// Only the leading `n-2k` rows must match: a print that scrolls in the
    /// same step (long wrapped line at the bottom) lands its new rows at the
    /// tail of the overlap, so the tail `k` overlap rows are excused. This
    /// biases toward preservation — a coincidental match archives lines to
    /// history rather than dropping them. Candidates beyond the batch's
    /// scroll bound are ignored: repetitive content can match any shift,
    /// but the batch itself caps how far the grid could have moved. A
    /// partial scroll still needs a
    /// non-blank line on either side of the fold so an idle blank screen
    /// never churns history; a fully blanked grid over non-blank rows
    /// counts as a full scroll-off (this preserves cleared content in
    /// history instead of dropping it). An identical grid never scrolls
    /// (covers no-op feeds on uniform screens, which would otherwise flood
    /// history).
    fn scroll_by(&self, grid: &[String], bound: usize) -> Option<usize> {
        let n = grid.len();
        if n == 0 || self.screen.len() != n {
            return None;
        }
        if self
            .screen
            .iter()
            .map(|l| l.text.as_str())
            .eq(grid.iter().map(String::as_str))
        {
            return None;
        }
        let mut smallest: Option<usize> = None;
        let mut evidenced: Option<usize> = None;
        for k in 1..n {
            // Leading rows must match exactly; trailing `k` overlap rows
            // may hold freshly printed content. Needs at least one
            // verified row, and the batch must be able to explain the
            // shift (bounds repetitive-content matches).
            if n < 2 * k + 1 || k > bound {
                continue;
            }
            let moved = &self.screen[..k];
            if !(moved.iter().any(|l| !l.text.is_empty())
                || grid[n - k..].iter().any(|l| !l.is_empty()))
            {
                continue;
            }
            let kept = &self.screen[k..n - k];
            if !kept
                .iter()
                .map(|l| l.text.as_str())
                .eq(grid[..n - 2 * k].iter().map(String::as_str))
            {
                continue;
            }
            if smallest.is_none() {
                smallest = Some(k);
            }
            // Non-blank evidence proves the shift: blanks align with
            // anything, so the largest evidenced `k` is the exact scroll.
            if grid[..n - 2 * k].iter().any(|l| !l.is_empty()) {
                evidenced = Some(k);
            }
        }
        if let Some(k) = evidenced.or(smallest) {
            return Some(k);
        }
        if grid.iter().all(|l| l.is_empty()) && self.screen.iter().any(|l| !l.text.is_empty()) {
            return Some(n);
        }
        None
    }

    fn apply_scroll(&mut self, k: usize, grid: &[String]) {
        let k = k.min(self.screen.len());
        for line in self.screen.drain(..k) {
            self.history.push_back(line);
        }
        for text in grid.iter().skip(grid.len().saturating_sub(k)) {
            self.head += 1;
            self.screen.push(RingLine {
                seq: self.head,
                text: text.clone(),
            });
        }
        self.evict();
    }

    /// Grow or shrink the screen vec to the grid length. Shrinking moves
    /// non-blank top rows into history; growing takes fresh seqs.
    fn fit_len(&mut self, grid: &[String]) {
        while self.screen.len() < grid.len() {
            let text = grid[self.screen.len()].clone();
            self.head += 1;
            self.screen.push(RingLine {
                seq: self.head,
                text,
            });
        }
        if self.screen.len() > grid.len() {
            let drop = self.screen.len() - grid.len();
            for line in self.screen.drain(..drop) {
                if !line.text.is_empty() {
                    self.history.push_back(line);
                }
            }
            self.evict();
        }
    }

    /// Rows whose text changed keep their position but take a fresh seq, so
    /// a polling cursor observes the repaint exactly once and the ring never
    /// grows on repaint alone.
    fn refresh_in_place(&mut self, grid: &[String]) {
        for (row, text) in self.screen.iter_mut().zip(grid.iter()) {
            if row.text != *text {
                row.text = text.clone();
                self.head += 1;
                row.seq = self.head;
            }
        }
    }

    fn evict(&mut self) {
        // Capacity counts content lines; trailing blank grid padding is not
        // content and must not eat the scrollback budget.
        while self.history.len() + self.screen_content_len() > self.max_lines {
            if self.history.pop_front().is_none() {
                break;
            }
        }
    }

    /// Screen rows through the last non-blank one: trailing blank grid rows
    /// are padding, not content (matches `contents()` trimming newlines).
    fn screen_content_len(&self) -> usize {
        let mut n = self.screen.len();
        while n > 0 && self.screen[n - 1].text.is_empty() {
            n -= 1;
        }
        n
    }

    fn oldest_retained(&self) -> u64 {
        if let Some(first) = self.history.front() {
            return first.seq;
        }
        if let Some(min) = self.screen.iter().map(|l| l.seq).min() {
            return min;
        }
        self.head + 1
    }

    /// Content lines (history + live screen rows) as (seq, text) in
    /// positional order.
    fn content(&self) -> impl Iterator<Item = &RingLine> {
        let n = self.screen_content_len();
        self.history.iter().chain(self.screen.iter().take(n))
    }

    fn rendered(&self, after_seq: u64, max_lines: usize) -> RenderedRead {
        let picked: Vec<&RingLine> = self.content().filter(|l| l.seq > after_seq).collect();
        let total = picked.len();
        let take = max_lines.min(total);
        // Newest `take` by position; still cursor-continuous.
        let lines = &picked[total - take..];
        let text = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        RenderedRead {
            seq: lines.first().map(|l| l.seq).unwrap_or(self.head),
            next_seq: self.head,
            dropped: after_seq != 0
                && (after_seq <= self.dropped_floor
                    || after_seq < self.oldest_retained()
                    || after_seq > self.head),
            truncated: total > take,
            text,
        }
    }

    fn tail_vec(&self, n: usize, strip: bool) -> Vec<String> {
        let total = self.history.len() + self.screen_content_len();
        let skip = total.saturating_sub(n);
        let mut out: Vec<String> = self.content().skip(skip).map(|l| l.text.clone()).collect();
        if strip {
            out = out.iter().map(|l| strip_ansi(l)).collect();
        }
        out
    }

    fn ring_len(&self) -> usize {
        self.history.len() + self.screen_content_len()
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        if self.cols == cols && self.rows == rows {
            return;
        }
        // No reflow: vt100 truncates/pads rows in place, so the visible
        // screen survives (TUIs redraw on SIGWINCH anyway). Length fixup
        // happens on the next feed.
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

    /// Rendered lines after `after_seq` (oldest→newest), per contracts/ipc.md.
    pub fn rendered(&self, id: &str, after_seq: u64, max_lines: usize) -> Option<RenderedRead> {
        self.surfaces
            .get(id)
            .map(|s| s.rendered(after_seq, max_lines))
    }

    /// Last `n` content lines (oldest→newest), optionally ANSI-stripped.
    /// Rebased on the rendered ring: same shape as before, cursor-correct.
    pub fn tail(&self, id: &str, n: usize, strip: bool) -> Option<Vec<String>> {
        self.surfaces.get(id).map(|s| s.tail_vec(n, strip))
    }

    /// Retained content lines (history + live screen rows).
    pub fn ring_len(&self, id: &str) -> usize {
        self.surfaces.get(id).map(|s| s.ring_len()).unwrap_or(0)
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
    /// Sequences restart at 1, so every pre-restart cursor reads as dropped
    /// (see `dropped_floor`); content stays available from scratch.
    pub fn restore_surface(&mut self, id: &str, cols: u16, rows: u16, lines: Vec<String>) {
        let mut s = Surface::new(cols, rows);
        let skip = lines.len().saturating_sub(s.max_lines);
        for line in lines.into_iter().skip(skip) {
            s.head += 1;
            s.history.push_back(RingLine {
                seq: s.head,
                text: strip_ansi(&line),
            });
        }
        s.dropped_floor = s.head;
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
        b.feed_output("p", b"hello\r\nworld\r\n");
        assert!(b.snapshot("p").contains("hello"));
        let tail = b.tail("p", 10, false).unwrap();
        assert_eq!(tail, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn tail_keeps_utf8_split_across_reads() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"caf\xc3");
        b.feed_output("p", b"\xa9\r\n");
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
        b.feed_output("p", b"\x1b[31mred\x1b[0m\r\n");
        assert_eq!(b.tail("p", 5, true).unwrap(), vec!["red".to_string()]);
    }

    #[test]
    fn scrollback_is_bounded() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for i in 0..(DEFAULT_SCROLLBACK_LINES + 100) {
            b.feed_output("p", format!("line {i}\r\n").as_bytes());
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
        b.feed_output("p", b"Status: Starting\rStatus: Ready   \r\n");
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
        b.feed_output("p", b"=== hdr ===\r\nline A\r\n\x1b[1A\x1b[2Kline B\r\n");
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
        b.feed_output("p", b"hello\x1b[2K\x1b[Gbye\r\n");
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
        b.feed_output("p", b"alpha\r\n");
        let r1 = b.rendered("p", 0, 200).unwrap();
        assert!(r1.text.contains("alpha"));
        b.feed_output("p", b"beta\r\n");
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
            bulk.push_str(&format!("l{i}\r\n"));
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
                bulk.push_str(&format!("line {}\r\n", chunk * 100 + i));
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
            b.feed_output("p", format!("l{i}\r\n").as_bytes());
        }
        let r = b.rendered("p", 0, 3).unwrap();
        assert!(r.truncated);
        assert_eq!(r.text, "l7\nl8\nl9");
    }

    #[test]
    fn rendered_alt_screen_returns_grid_without_history_pollution() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"main line\r\n");
        let before = rendered_text(&b, "p");
        assert_eq!(before, "main line");
        // Enter alt screen: main content hidden, alt grid shown.
        b.feed_output("p", b"\x1b[?1049h");
        b.feed_output("p", b"alt content\r\n");
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
        b.feed_output("p", b"keep\r\n");
        let len_before = b.ring_len("p");
        b.feed_output("p", b"\x1b[2J\x1b[H");
        assert_eq!(b.ring_len("p"), len_before);
    }

    #[test]
    fn restore_marks_pre_restart_cursors_dropped() {
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"before\r\n");
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
        b.feed_output("p", b"keepme\r\n");
        b.resize("p", 100, 30);
        assert_eq!(b.tail("p", 5, true).unwrap(), vec!["keepme".to_string()]);
    }
}

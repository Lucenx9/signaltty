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

/// How far one batch could have moved the grid.
///
/// `linefeeds` counts motions that advance a row (LF/VT/FF/NEL, CSI `S`/`T`,
/// and printable wraps). `bound` is that count plus one slack, and plus a
/// large allowance for ED (`CSI J`), which blanks the screen without moving
/// the cursor. Unevidenced scroll matches may use only `linefeeds` from the
/// pre-batch cursor; the slack and an erase do not invent a scroll.
#[derive(Clone, Copy)]
struct ScrollBudget {
    bound: usize,
    linefeeds: usize,
}

fn scroll_budget(batch: &[u8], cols: u16) -> ScrollBudget {
    let mut linefeeds = 0usize;
    let mut clear = 0usize;
    let mut printable = 0usize;
    let mut i = 0;
    while i < batch.len() {
        let b = batch[i];
        match b {
            b'\n' | b'\x0b' | b'\x0c' | b'\x84' => linefeeds += 1,
            b'\x1b' => {
                let mut j = i + 1;
                if j < batch.len() && batch[j] == b'E' {
                    linefeeds += 1; // NEL
                    i = j;
                } else if j < batch.len() && batch[j] == b'[' {
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
                        linefeeds += if digits { val.max(1) } else { 1 };
                        i = j;
                    } else if j < batch.len() && batch[j] == b'J' {
                        // ED can blank every row. Not a cursor linefeed.
                        clear = 4096;
                        i = j;
                    }
                }
            }
            0x20..=0x7e | 0x80..=0xff => printable += 1,
            _ => {}
        }
        i += 1;
    }
    linefeeds += printable / (cols.max(1) as usize);
    ScrollBudget {
        bound: linefeeds.saturating_add(1).saturating_add(clear),
        linefeeds,
    }
}

/// Fast byte: printable ASCII or a line-advancing control that vt100 maps
/// to plain cursor motion with no mode dependence (CR resets col;
/// LF/VT/FF all funnel into the same `lf()`).
fn is_fast_byte(b: u8) -> bool {
    matches!(b, 0x20..=0x7e | b'\r' | b'\n' | 0x0b | 0x0c)
}

/// A legacy batch is fast-eligible iff every byte is fast. (Ground state,
/// clean UTF-8 and sticky modes are checked separately per batch.)
fn batch_is_plain(batch: &[u8]) -> bool {
    batch.iter().all(|&b| is_fast_byte(b))
}

/// Build a ring line from an ASCII row buffer, trimming trailing padding.
/// Equivalent to `str::trim_end` here: the buffer holds ASCII printables
/// and space padding only (tabs never lodge in grid cells as content).
/// Returns None on non-ASCII content — the caller aborts to the slow path.
fn ascii_line(buf: &[u8]) -> Option<String> {
    let mut end = buf.len();
    while end > 0 && buf[end - 1] == b' ' {
        end -= 1;
    }
    String::from_utf8(buf[..end].to_vec()).ok()
}

/// vte ground-state model for fast-run eligibility. Mirrors the vte 0.11.1
/// state table for the bytes that can appear in a feed: ESC aborts any
/// state (Anywhere rule), CAN/SUB abort, C0 controls preserve state, C1
/// bytes never change state (except 0x9c out of DCS/SOS), CSI completes on
/// 0x40–0x7e, OSC completes on BEL, DCS/SOS complete on 0x9c or ST.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ScanState {
    #[default]
    Ground,
    Esc,
    EscInter,
    Csi,
    Osc,
    Dcs,
}

/// utf8parse 0.2.2 model (exact positional validity, not just a counter: a
/// wrong-range continuation is an error back to Ground, which a counter
/// would mistrack as still-pending).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Utf8State {
    #[default]
    Ground,
    Tail1,
    Tail2,
    Tail3,
    NeedE0,
    NeedED,
    NeedF0,
    NeedF4,
}

fn step_utf8(st: Utf8State, b: u8) -> Utf8State {
    match st {
        Utf8State::Ground | Utf8State::Tail1 => Utf8State::Ground,
        Utf8State::Tail2 => {
            if (0x80..=0xbf).contains(&b) {
                Utf8State::Tail1
            } else {
                Utf8State::Ground
            }
        }
        Utf8State::Tail3 => {
            if (0x80..=0xbf).contains(&b) {
                Utf8State::Tail2
            } else {
                Utf8State::Ground
            }
        }
        Utf8State::NeedE0 => {
            if (0xa0..=0xbf).contains(&b) {
                Utf8State::Tail1
            } else {
                Utf8State::Ground
            }
        }
        Utf8State::NeedED => {
            if (0x80..=0x9f).contains(&b) {
                Utf8State::Tail1
            } else {
                Utf8State::Ground
            }
        }
        Utf8State::NeedF0 => {
            if (0x90..=0xbf).contains(&b) {
                Utf8State::Tail2
            } else {
                Utf8State::Ground
            }
        }
        Utf8State::NeedF4 => {
            if (0x80..=0x8f).contains(&b) {
                Utf8State::Tail2
            } else {
                Utf8State::Ground
            }
        }
    }
}

/// Partial CSI being accumulated for sticky-mode detection. Bounded state
/// (saturating numbers, two stored params): carries cleanly across feeds.
/// Disagreements with vte's param parsing on exotic shapes (leading colons
/// and the like) always fall slow-ward, never fast.
#[derive(Clone, Copy, Debug, Default)]
struct CsiAccum {
    private: bool,
    inter: bool,
    p0: u32,
    p1: u32,
    nparams: u8,
    cur: u32,
    has_cur: bool,
    subs: u8,
    first_sub: u32,
    seen_6: bool,
}

impl CsiAccum {
    fn digit(&mut self, d: u8) {
        self.cur = self
            .cur
            .saturating_mul(10)
            .saturating_add(d as u32)
            .min(9999);
        self.has_cur = true;
    }

    /// Close the current subparam (`:` separator, `;` separator, or the
    /// final byte). Slots the first sub of each of the first two params
    /// (vt100's DECSTBM canonicalization reads exactly those).
    fn sub_close(&mut self) {
        let val = if self.has_cur { self.cur } else { 0 };
        if self.subs == 0 {
            self.first_sub = val;
            if self.nparams < 2 {
                if self.nparams == 0 {
                    self.p0 = val;
                } else {
                    self.p1 = val;
                }
            }
        }
        if self.has_cur {
            self.subs = self.subs.saturating_add(1);
        }
        self.cur = 0;
        self.has_cur = false;
    }

    /// Close the current `;`-separated param, recording an exact `[6]`
    /// (mirrors decset's `&[6]` arm).
    fn param_close(&mut self) {
        self.sub_close();
        if self.subs == 1 && self.first_sub == 6 {
            self.seen_6 = true;
        }
        self.subs = 0;
        self.first_sub = 0;
        if self.nparams < 2 {
            self.nparams += 1;
        }
    }
}

/// Carry from one feed to the next: everything the scanner needs to seed
/// its left-to-right walk. All bounded, all Copy.
#[derive(Clone, Copy, Debug, Default)]
struct ParseCarry {
    state: ScanState,
    csi: CsiAccum,
    utf8: Utf8State,
}

/// A fast run's pending commit, recorded purely then applied after the
/// cursor check passes.
#[allow(clippy::enum_variant_names)]
enum FastOp {
    Refresh { row: usize, text: String },
    Scroll { line: String },
}

/// Advance the scan model by one slow byte. Fast bytes never reach here
/// (they are state-preserving by construction).
#[allow(clippy::too_many_arguments)]
fn step_scan_byte(
    b: u8,
    rows: u16,
    st: &mut ScanState,
    csi: &mut CsiAccum,
    utf8: &mut Utf8State,
    margins: &mut bool,
    origin: &mut bool,
) {
    // CAN/SUB abort any sequence (vte Anywhere rule).
    if b == 0x18 || b == 0x1a {
        *st = ScanState::Ground;
        *utf8 = Utf8State::Ground;
        return;
    }
    // UTF-8 layer first: vte checks Utf8 state before anything else, and a
    // non-continuation byte is consumed as U+FFFD back to Ground (this
    // swallows even ESC, so groundness must not be assumed after it).
    if *utf8 != Utf8State::Ground {
        *utf8 = step_utf8(*utf8, b);
        return;
    }
    // ESC aborts any state (vte Anywhere rule).
    if b == 0x1b {
        *st = ScanState::Esc;
        return;
    }
    match *st {
        ScanState::Ground => match b {
            0xc2..=0xdf => *utf8 = Utf8State::Tail1,
            0xe0 => *utf8 = Utf8State::NeedE0,
            0xe1..=0xec => *utf8 = Utf8State::Tail2,
            0xed => *utf8 = Utf8State::NeedED,
            0xee..=0xef => *utf8 = Utf8State::Tail2,
            0xf0 => *utf8 = Utf8State::NeedF0,
            0xf1..=0xf3 => *utf8 = Utf8State::Tail3,
            0xf4 => *utf8 = Utf8State::NeedF4,
            // Printables, C0 controls and C1 bytes: Print/Execute/ignore,
            // all state-preserving.
            _ => {}
        },
        ScanState::Esc => match b {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f | 0x80..=0x9f => {}
            0x20..=0x2f => *st = ScanState::EscInter,
            0x5b => {
                *st = ScanState::Csi;
                *csi = CsiAccum::default();
            }
            0x5d => *st = ScanState::Osc,
            0x50 | 0x58 | 0x5e | 0x5f => *st = ScanState::Dcs,
            // RIS rebuilds vt100's screen: sticky modes are gone.
            0x63 => {
                *st = ScanState::Ground;
                *margins = false;
                *origin = false;
            }
            _ => *st = ScanState::Ground,
        },
        ScanState::EscInter => {
            if (0x30..=0x7e).contains(&b) {
                *st = ScanState::Ground;
            }
        }
        ScanState::Csi => match b {
            0x30..=0x39 => csi.digit(b - b'0'),
            0x3a => csi.sub_close(),
            0x3b => csi.param_close(),
            0x3c..=0x3f => csi.private = true,
            0x20..=0x2f => csi.inter = true,
            0x40..=0x7e => {
                csi.param_close();
                csi_dispatch_accum(csi, b, rows, margins, origin);
                *st = ScanState::Ground;
            }
            // C0 controls execute without disturbing CSI; DEL and C1 bytes
            // are ignored in CSI states (vte tables have no entries).
            _ => {}
        },
        // BEL terminates OSC (vte OscString table). 0x9c notably does
        // NOT (no table entry: consumed as data).
        ScanState::Osc => {
            if b == 0x07 {
                *st = ScanState::Ground;
            }
        }
        ScanState::Dcs => {
            if b == 0x9c {
                *st = ScanState::Ground;
            }
        }
    }
}

/// Apply a completed CSI to the sticky flags.
fn csi_dispatch_accum(
    csi: &CsiAccum,
    final_byte: u8,
    rows: u16,
    margins: &mut bool,
    origin: &mut bool,
) {
    // vt100 ignores any dispatch with intermediates (`Some(i)` arm).
    if csi.inter {
        return;
    }
    match final_byte {
        b'r' if !csi.private => {
            // DECSTBM. Mirrors grid::set_scroll_region via decstbm: params
            // default to full screen, and an inverted range resets to full.
            let top = if csi.p0 == 0 { 1 } else { csi.p0 };
            let bottom = if csi.p1 == 0 { rows as u32 } else { csi.p1 };
            let rows_u = rows as u32;
            // Full-screen params (or an inverted range, which vt100
            // resets to full) deactivate the region; anything else sticks.
            let full = top == 1 && bottom == rows_u;
            let inverted = top.saturating_sub(1) >= bottom.saturating_sub(1);
            *margins = !(full || inverted);
        }
        b'h' | b'l' if csi.private => {
            // DECSET/DECRST `?6` (origin mode). Deliberately never cleared
            // except by RIS: DECRC can restore a saved origin mode without
            // a visible `6`, so clearing on `?6l` would be unsound.
            if csi.seen_6 {
                *origin = true;
            }
        }
        _ => {}
    }
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
    /// Fast-path parser model. `feed()` carves plain-ASCII runs out of the
    /// byte stream and captures their lines from the input side with zero
    /// grid extraction; everything else goes through the legacy grid
    /// reconcile below. The model mirrors the `vte` state machine (ground
    /// detection) plus the two pieces of sticky `vt100` mode that affect
    /// plain-text layout (scroll region, origin mode). See `feed`.
    carry: ParseCarry,
    /// True once any feed left the parser outside ground state or set a
    /// sticky mode: while set, plain runs take the slow path. Cleared only
    /// by RIS (`ESC c`, which rebuilds vt100's screen from scratch).
    margins_sticky: bool,
    origin_sticky: bool,
    /// Set by `resize`: the grid was truncated/padded in place, so cached
    /// row texts may be stale. Forces one slow reconcile (which resyncs via
    /// `fit_len` + `refresh_in_place`); cleared there.
    resized: bool,
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
            carry: ParseCarry::default(),
            margins_sticky: false,
            origin_sticky: false,
            resized: false,
        }
    }

    fn feed(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        // Degenerate sizes and single-row screens take the legacy path:
        // the fast row model below needs at least two rows (a scroll must
        // leave the completed row behind on the grid).
        if self.rows < 2 || self.cols < 1 {
            self.feed_slow(data);
            #[cfg(debug_assertions)]
            self.debug_assert_grid_sync();
            return;
        }
        if self.screen.is_empty() {
            // Factory-fresh parser: blank grid, cursor at (0,0). Size the
            // screen vec blindly; simulation and reconciles fill it in.
            for _ in 0..self.rows {
                self.screen.push(RingLine {
                    seq: 0,
                    text: String::new(),
                });
            }
        }
        // Walk the feed on the legacy cut points (CR/LF + byte budget),
        // classifying each batch: plain batches in Ground state fuse into
        // pending fast spans (zero grid reads), anything else flushes the
        // span and reconciles exactly as the legacy loop would. Slow batches
        // are never split or fused, so every legacy reconcile observes the
        // same grid state at the same byte offset as before.
        let max_bytes = (self.cols as usize).max(64);
        let mut st = self.carry.state;
        let mut csi = self.carry.csi;
        let mut utf8 = self.carry.utf8;
        let mut margins = self.margins_sticky;
        let mut origin = self.origin_sticky;
        // Pending fast span [span_start..i+1): consecutive fast-eligible
        // legacy batches, not yet fed to the parser.
        let mut span_start: Option<usize> = None;
        let mut start = 0usize;
        for (i, &b) in data.iter().enumerate() {
            let cut = b == b'\n' || b == b'\r';
            let bytes = i + 1 - start;
            if !(cut || bytes >= max_bytes || i + 1 == data.len()) {
                continue;
            }
            let batch = &data[start..i + 1];
            let eligible = st == ScanState::Ground
                && utf8 == Utf8State::Ground
                && !margins
                && !origin
                && batch_is_plain(batch);
            if eligible {
                // Fuse: fast bytes preserve all scan state, so stepping is
                // skipped (and the span stays eligible by induction).
                if span_start.is_none() {
                    span_start = Some(start);
                }
            } else {
                if let Some(ss) = span_start.take() {
                    if !self.try_fast(&data[ss..start]) {
                        self.feed_slow(&data[ss..start]);
                    }
                }
                let budget = scroll_budget(batch, self.cols);
                let pre_row = self.parser.screen().cursor_position().0 as usize;
                self.parser.process(batch);
                self.reconcile(budget, pre_row, margins);
                for &sb in batch {
                    step_scan_byte(
                        sb,
                        self.rows,
                        &mut st,
                        &mut csi,
                        &mut utf8,
                        &mut margins,
                        &mut origin,
                    );
                }
            }
            start = i + 1;
        }
        if let Some(ss) = span_start.take() {
            if !self.try_fast(&data[ss..]) {
                self.feed_slow(&data[ss..]);
            }
        }
        self.carry = ParseCarry {
            state: st,
            csi,
            utf8,
        };
        self.margins_sticky = margins;
        self.origin_sticky = origin;
        #[cfg(debug_assertions)]
        self.debug_assert_grid_sync();
    }

    /// Legacy path, verbatim: reconcile the rendered ring in small batches
    /// so a bulk scroll that lands in one PTY read is still captured line
    /// by line. Batches cut after every CR/LF: printing a line and
    /// scrolling it off are then reconciled as the two separate steps the
    /// terminal applied, which keeps strict scroll matching exact (a
    /// print+scroll in one step would hide the new line mid-grid and
    /// defeat the matcher).
    /// Splitting on CR/LF is UTF-8 safe (neither byte appears inside a
    /// multibyte sequence) and harmless to the parser, which is a byte
    /// state machine that resumes across `process` calls — even mid-OSC.
    /// The byte budget (~one visual row) bounds how much new content a
    /// single reconcile can carry, so wrap-driven scrolls stay inside
    /// the matcher's tail allowance.
    fn feed_slow(&mut self, data: &[u8]) {
        let max_bytes = (self.cols as usize).max(64);
        let mut start = 0usize;
        for (i, &b) in data.iter().enumerate() {
            let cut = b == b'\n' || b == b'\r';
            let bytes = i + 1 - start;
            if cut || bytes >= max_bytes || i + 1 == data.len() {
                let batch = &data[start..i + 1];
                let budget = scroll_budget(batch, self.cols);
                let pre_row = self.parser.screen().cursor_position().0 as usize;
                self.parser.process(batch);
                start = i + 1;
                self.reconcile(budget, pre_row, self.margins_sticky);
            }
        }
    }

    // ---------------- fast path: input-side line capture ----------------
    //
    // Grid extraction (`grid_text`) costs ~42µs per 24-row screen; at ~26k
    // CR/LF batches per MiB that dominated `feed()` (~1.1s/MiB vs ~13ms of
    // raw vt100 parsing). The fast path carves plain-ASCII runs (printable
    // ASCII + CR/LF/VT/FF, nothing else) out of the feed and captures their
    // lines from the input side with zero grid reads. Everything else —
    // escape sequences, controls, UTF-8, alt-screen, sticky modes — takes
    // the legacy grid reconcile, verbatim.
    //
    // Soundness argument (all verified against vt100 0.15.2 + vte 0.11.1 +
    // utf8parse 0.2.2 sources):
    // - A fast run starts only in vte Ground state with no partial UTF-8
    //   (the feed scan tracks both exactly, including carry across feeds;
    //   ESC aborts any state, CAN/SUB abort, C0 controls preserve state,
    //   C1 bytes never change state except ST-after-DCS/SOS, UTF-8 errors
    //   consume one byte back to Ground). Fast bytes themselves are
    //   state-preserving (Print/Execute), so groundness survives the run.
    // - vt100 ignores the modes that would otherwise complicate plain-text
    //   layout: no LNM (`lf` never resets col), no IRM (`sm`/`rm` are
    //   no-ops), no wrap suppression (`?7` unhandled), no charset shifts
    //   (SO/SI are no-ops). The two modes it honors and that affect plain
    //   feeds — scroll region (`CSI r`) and origin mode (`CSI ? 6 h`) — are
    //   tracked as sticky flags that force the slow path (cleared only by
    //   RIS, which rebuilds the screen). Alt-screen is checked per run.
    // - The row simulation mirrors vt100 exactly for ASCII: width-1 cells,
    //   wrap when col == cols before a printable (same `col_wrap` rule; the
    //   last-cell wrap flag only affects `contents()` joining, never rows),
    //   CR resets col, LF/VT/FF increment the row with a scroll at the
    //   bottom margin. Touched rows must hold ASCII (byte columns); any
    //   other row aborts the run to the slow path.
    // - Belt and braces: the simulated end cursor must equal the parser's
    //   real cursor, or the run reconciles from grid truth once (never
    //   re-feeds: the bytes are already consumed, so a second `process`
    //   would duplicate them). Debug builds additionally assert full grid
    //   sync after every fast feed.
    //
    // Intended behavior changes vs the legacy matcher (both unpinned by any
    // test, both fixes): repetitive streams no longer duplicate lines (the
    // matcher's bound admitted a spurious k=2 scroll: 30×"y" became 41
    // lines), and blank scrolled lines are captured as empty entries instead
    // of vanishing through the blank-scroll quirk.

    /// Capture one plain-ASCII run from the input side.
    ///
    /// Exactly-once contract: every byte is processed by the parser
    /// exactly once. All bail-out checks that don't need post-state run
    /// BEFORE `process` and return false (the parser is untouched, so the
    /// caller may run the legacy path over the same bytes). Once `process`
    /// runs this function never returns false: a cursor mismatch reconciles
    /// from the grid once instead of re-feeding, so a second `process` of
    /// the same bytes is impossible (a prior version processed first and
    /// returned false afterwards, double-feeding the parser whenever a
    /// touched row held non-ASCII text or the cursors diverged). The screen
    /// vec is untouched until the run is committed or reconciled.
    fn try_fast(&mut self, run: &[u8]) -> bool {
        if run.is_empty() {
            return true;
        }
        if self.screen.len() != self.rows as usize || self.resized {
            return false;
        }
        if self.in_alt || self.margins_sticky || self.origin_sticky {
            return false;
        }
        let cols = self.cols as usize;
        let rows = self.rows as usize;
        let (start_row, start_col) = self.parser.screen().cursor_position();
        let (mut row, mut col) = (start_row as usize, start_col as usize);
        if row >= rows || col > cols {
            return false;
        }
        // Pure simulation over the cached screen (grid-synced by induction:
        // every prior segment committed or reconciled exactly). Runs BEFORE
        // `process` so every ASCII precondition bails with the parser
        // untouched and the caller can safely take the legacy path.
        if !self.screen[row].text.is_ascii() {
            return false;
        }
        let mut buf: Vec<u8> = self.screen[row].text.clone().into_bytes();
        let mut ops: Vec<FastOp> = Vec::new();
        macro_rules! complete_row {
            () => {{
                let line = match ascii_line(buf.as_slice()) {
                    Some(line) => line,
                    None => return false,
                };
                if row + 1 >= rows {
                    ops.push(FastOp::Scroll { line });
                    row = rows - 1;
                    // The scrolled-in row is blank by terminal semantics
                    // (vt100 inserts a fresh row); it must NOT be reloaded
                    // from the screen vec, which still holds the pre-shift
                    // bottom row until ops apply.
                    buf = Vec::new();
                } else {
                    ops.push(FastOp::Refresh { row, text: line });
                    row += 1;
                    // No shift happened, so the cached row is valid.
                    if !self.screen[row].text.is_ascii() {
                        return false;
                    }
                    buf = self.screen[row].text.clone().into_bytes();
                }
                // col is intentionally preserved: vt100 has no LNM, so
                // LF/VT/FF never touch the column (staircase is correct).
            }};
        }
        for &b in run {
            match b {
                b'\r' => col = 0,
                b'\n' | 0x0b | 0x0c => complete_row!(),
                _ => {
                    debug_assert!((0x20..=0x7e).contains(&b), "fast run holds printables only");
                    if col >= cols {
                        // vt100 `col_wrap`: wrap before writing when the
                        // cursor sits past the last column.
                        complete_row!();
                        col = 0;
                    }
                    if col < buf.len() {
                        // ASCII-only buffer: byte index == column.
                        buf[col] = b;
                    } else {
                        while buf.len() < col {
                            buf.push(b' ');
                        }
                        buf.push(b);
                    }
                    col += 1;
                }
            }
        }
        let final_text = match ascii_line(buf.as_slice()) {
            Some(text) => text,
            None => return false,
        };
        // Single parse of these bytes: the parser owns ground truth now.
        self.parser.process(run);
        let (end_row, end_col) = self.parser.screen().cursor_position();
        if row != end_row as usize || col != end_col as usize {
            // Model divergence (untracked mode or vt100 behavior change):
            // sync the ring from grid truth once. Never re-feed: the bytes
            // are already consumed, and a second `process` would duplicate
            // them in the grid. History is untouched so far.
            let budget = scroll_budget(run, self.cols);
            self.reconcile(budget, start_row as usize, self.margins_sticky);
            return true;
        }
        ops.push(FastOp::Refresh {
            row,
            text: final_text,
        });
        for op in ops {
            match op {
                FastOp::Refresh { row, text } => {
                    let slot = &mut self.screen[row];
                    if slot.text != text {
                        self.head += 1;
                        slot.seq = self.head;
                        slot.text = text;
                    }
                }
                FastOp::Scroll { line } => {
                    let top = self.screen.remove(0);
                    // The completed line stays visible one row up; sync its
                    // text, bumping only on change (same rule as refresh).
                    let staying = &mut self.screen[rows - 2];
                    if staying.text != line {
                        self.head += 1;
                        staying.seq = self.head;
                        staying.text = line;
                    }
                    self.history.push_back(top);
                    // A scroll always inserts a blank row; the legacy path
                    // mints it a fresh sequence unconditionally.
                    self.head += 1;
                    self.screen.push(RingLine {
                        seq: self.head,
                        text: String::new(),
                    });
                    self.evict();
                }
            }
        }
        true
    }

    /// Debug-only: the screen vec must mirror the grid exactly.
    #[cfg(debug_assertions)]
    fn debug_assert_grid_sync(&self) {
        let grid = self.grid_text();
        assert_eq!(
            grid.len(),
            self.screen.len(),
            "screen vec length drifted from grid"
        );
        for (i, (g, s)) in grid.iter().zip(self.screen.iter()).enumerate() {
            assert_eq!(g, &s.text, "screen vec row {i} drifted from grid");
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

    fn reconcile(&mut self, budget: ScrollBudget, pre_row: usize, in_region: bool) {
        let alt = self.parser.screen().alternate_screen();
        let grid = self.grid_text();
        let transitioned = alt != self.in_alt;
        self.in_alt = alt;
        self.resized = false;
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
            if let Some(k) = self.scroll_by(&grid, budget, pre_row, in_region) {
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
    /// counts as a full scroll-off only when the batch's bound can explain
    /// moving every row (an explicit clear — `scroll_budget` reports ED as
    /// a large bound). A short overwrite that leaves the grid blank is an
    /// in-place edit, not a scroll. An identical grid never scrolls
    /// (covers no-op feeds on uniform screens, which would otherwise flood
    /// history). `pre_row` is the cursor row before the batch: on the full
    /// screen a linefeed scrolls only from the bottom row. Inside a scroll
    /// region the bottom is not the screen bottom, so that cap is skipped.
    fn scroll_by(
        &self,
        grid: &[String],
        budget: ScrollBudget,
        pre_row: usize,
        in_region: bool,
    ) -> Option<usize> {
        let n = grid.len();
        let bound = budget.bound;
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
        // A non-blank kept prefix proves the shift. Without that evidence,
        // blanks match every `k`. Trust an unevidenced shift only when the
        // cursor, plus this batch's linefeeds, could have reached the
        // bottom and scrolled that far. An erase followed by LF from a
        // higher row blanks the line in place.
        let max_scroll = if in_region {
            bound
        } else {
            pre_row
                .saturating_add(budget.linefeeds)
                .saturating_sub(n.saturating_sub(1))
        };
        if let Some(k) = evidenced.or(smallest.filter(|&k| k <= max_scroll && k < bound)) {
            return Some(k);
        }
        if bound >= n
            && grid.iter().all(|l| l.is_empty())
            && self.screen.iter().any(|l| !l.text.is_empty())
        {
            return Some(n);
        }
        None
    }

    fn apply_scroll(&mut self, k: usize, grid: &[String]) {
        // A full-screen clear matches `k == n` with an empty grid. Archive
        // the non-blank lines only — blank padding must not flood history.
        if k >= self.screen.len() && grid.iter().all(|l| l.is_empty()) {
            for line in self.screen.drain(..) {
                if !line.text.is_empty() {
                    self.history.push_back(line);
                }
            }
            self.fit_len(grid);
            self.evict();
            return;
        }
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
        self.resized = true;
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

    // ---- Fast path (input-side capture): behavior pins ----

    #[test]
    fn fast_repetitive_lines_captured_exactly() {
        // The legacy matcher's scroll bound admitted a spurious k=2 shift
        // on repetitive content (30x"y" became 41 lines). The fast path
        // captures exactly.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for _ in 0..30 {
            b.feed_output("p", b"y\r\n");
        }
        let r = b.rendered("p", 0, 5000).unwrap();
        assert_eq!(r.text.lines().count(), 30);
        assert!(r.text.lines().all(|l| l == "y"));
    }

    #[test]
    fn fast_blank_scroll_preserves_scrolled_line() {
        // A blank LF on a full screen scrolls exactly one row: the top
        // line must move to history, not vanish through the legacy
        // matcher's blank-scroll quirk (which dropped it).
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        for i in 0..30 {
            b.feed_output("p", format!("line{i}\r\n").as_bytes());
        }
        b.feed_output("p", b"\r\n");
        let text = rendered_text(&b, "p");
        assert!(
            text.contains("line6"),
            "scrolled-off top line kept: {text:?}"
        );
        assert_eq!(text.lines().count(), 30);
    }

    #[test]
    fn fast_long_wrapped_line_bit_for_bit() {
        // 500 ASCII chars = 6 full rows + 20 cells, no scroll on 24 rows.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        let line = "A".repeat(500);
        b.feed_output("p", format!("{line}\r\n").as_bytes());
        let tail = b.tail("p", 8, false).unwrap();
        assert_eq!(tail.len(), 7);
        for row in &tail[..6] {
            assert_eq!(row, &"A".repeat(80));
        }
        assert_eq!(tail[6], "A".repeat(20));
    }

    #[test]
    fn fast_staircase_n_without_r() {
        // Bare LF never resets the column (vt100 has no LNM): cooked PTYs
        // send CRLF, but raw printf output staircases like a real terminal.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"ab\ncd\n");
        assert_eq!(
            b.tail("p", 2, false).unwrap(),
            vec!["ab".to_string(), "  cd".to_string()]
        );
    }

    #[test]
    fn fast_split_sequence_stays_sound() {
        // An SGR split across feeds must not leak param bytes into content:
        // the second feed's leading bytes complete the CSI, then "B" prints.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"A\x1b[31");
        b.feed_output("p", b"mB\r\n");
        assert_eq!(rendered_text(&b, "p"), "AB");
    }

    #[test]
    fn fast_split_csi_across_feeds_recovers_ground() {
        // ED split across feeds: completion dispatches, ground returns, and
        // the following plain text takes the fast path again.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"keep\r\n\x1b[2");
        b.feed_output("p", b"Jafter\r\n");
        let text = rendered_text(&b, "p");
        assert!(text.contains("after"), "{text:?}");
        assert!(
            !text.contains('J'),
            "CSI final must dispatch, not print: {text:?}"
        );
    }

    #[test]
    fn fast_sticky_scroll_region_stays_correct() {
        // A scroll region forces plain runs onto the slow path (grid
        // truth). DECSTBM homes the cursor, so "b" overwrites "a": that
        // overwrite proves the sticky flag engaged (the fast path would
        // have placed "b" at row 1). RIS clears the flag again.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"a\r\n");
        b.feed_output("p", b"\x1b[1;5r");
        b.feed_output("p", b"b\r\n");
        let text = rendered_text(&b, "p");
        assert_eq!(text, "b", "{text:?}");
        b.feed_output("p", b"\x1bc");
        b.feed_output("p", b"c\r\n");
        assert!(rendered_text(&b, "p").contains('c'));
    }

    #[test]
    fn fast_sticky_origin_mode_stays_correct() {
        // Origin-set homes the cursor, so "y" overwrites "x": same proof
        // as above, the fast path would have read "x\\ny".
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"x\r\n");
        b.feed_output("p", b"\x1b[?6h");
        b.feed_output("p", b"y\r\n");
        let text = rendered_text(&b, "p");
        assert_eq!(text, "y", "{text:?}");
    }

    #[test]
    fn fast_chunked_feeds_capture_exactly() {
        // PTY reads split lines arbitrarily (odd chunk sizes realign digit
        // widths mid-stream). Every line must land exactly once, in order,
        // regardless of fragmentation. Regression test: a stale row-buffer
        // reload after an in-span scroll once corrupted these.
        for chunk in [2usize, 3, 7, 101] {
            let mut data = Vec::new();
            for i in 1..=300 {
                data.extend_from_slice(format!("line{i}\r\n").as_bytes());
            }
            let mut b = HeadlessBackend::new();
            b.create_surface("p", 80, 24);
            for piece in data.chunks(chunk) {
                b.feed_output("p", piece);
            }
            let r = b.rendered("p", 0, 100_000).unwrap();
            let expect: Vec<String> = (1..=300).map(|i| format!("line{i}")).collect();
            assert_eq!(
                r.text.lines().collect::<Vec<_>>(),
                expect.iter().map(String::as_str).collect::<Vec<_>>(),
                "chunk={chunk}"
            );
        }
    }

    // ---- Exactly-once oracle: independent single vt100 parse ----

    /// Fresh vt100 parser fed the FULL input stream exactly once (a single
    /// `process` call). Never shares state with the Surface under test, so
    /// any double-feed in the ring (parser or ring duplication) diverges
    /// from it. The pre-existing `debug_assert_grid_sync` compares the ring
    /// to the Surface's own (possibly double-fed) parser, so it cannot see
    /// this class of bug; these helpers can.
    fn oracle_parse(full: &[u8], cols: u16, rows: u16) -> vt100::Parser {
        let mut p = vt100::Parser::new(rows, cols, 0);
        p.process(full);
        p
    }

    fn oracle_grid(full: &[u8], cols: u16, rows: u16) -> Vec<String> {
        let p = oracle_parse(full, cols, rows);
        let width = cols.max(1);
        p.screen()
            .rows(0, width)
            .map(|r| r.trim_end().to_string())
            .collect()
    }

    fn oracle_contents(full: &[u8], cols: u16, rows: u16) -> String {
        oracle_parse(full, cols, rows).screen().contents()
    }

    /// Oracle ring text for inputs that never scroll: the oracle grid rows
    /// with per-row trailing padding trimmed and trailing blank rows
    /// dropped — the same normalization the ring applies (`grid_text` +
    /// `screen_content_len`). (`vt100::Screen::contents` keeps trailing
    /// padding spaces, so it cannot be compared to ring text directly.)
    fn oracle_text(full: &[u8], cols: u16, rows: u16) -> String {
        let grid = oracle_grid(full, cols, rows);
        let mut n = grid.len();
        while n > 0 && grid[n - 1].is_empty() {
            n -= 1;
        }
        grid[..n].join("\n")
    }

    fn surface_screen_texts(b: &HeadlessBackend, id: &str) -> Vec<String> {
        b.surfaces
            .get(id)
            .unwrap()
            .screen
            .iter()
            .map(|l| l.text.clone())
            .collect()
    }

    #[test]
    fn fast_utf8_then_ascii_never_double_feeds() {
        // Exact repro from review: "café" rides the slow path (non-ASCII)
        // and leaves the cursor on a non-ASCII row; the following
        // pure-ASCII run used to be processed by `try_fast` and then
        // re-fed by `feed_slow`, duplicating the reads in both grid and
        // ring ("café OK\nnext\n OK\nnext").
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", "café".as_bytes());
        b.feed_output("p", " OK\r\nnext\r\n".as_bytes());
        let full = "café OK\r\nnext\r\n";
        assert_eq!(rendered_text(&b, "p"), "café OK\nnext");
        assert_eq!(b.snapshot("p"), oracle_contents(full.as_bytes(), 80, 24));
        assert_eq!(
            surface_screen_texts(&b, "p"),
            oracle_grid(full.as_bytes(), 80, 24)
        );
        // Same bytes in a single feed must agree too.
        let mut one = HeadlessBackend::new();
        one.create_surface("p", 80, 24);
        one.feed_output("p", full.as_bytes());
        assert_eq!(rendered_text(&one, "p"), "café OK\nnext");
        assert_eq!(one.snapshot("p"), oracle_contents(full.as_bytes(), 80, 24));
        assert_eq!(
            surface_screen_texts(&one, "p"),
            oracle_grid(full.as_bytes(), 80, 24)
        );
    }

    #[test]
    fn fast_ascii_crossing_utf8_row_never_double_feeds() {
        // Later-row bail: cursor is on an ASCII row, the next row is
        // non-ASCII. A plain run that line-feeds onto it used to be
        // parsed, rejected, and parsed again.
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"hello\r\ncaf\xc3\xa9\r\n");
        b.feed_output("p", b"\x1b[H");
        b.feed_output("p", b"Z\r\nMORE\r\n");
        let full: &[u8] = b"hello\r\ncaf\xc3\xa9\r\n\x1b[HZ\r\nMORE\r\n";
        assert_eq!(rendered_text(&b, "p"), "Zello\nMORE");
        assert_eq!(b.snapshot("p"), oracle_contents(full, 80, 24));
        assert_eq!(surface_screen_texts(&b, "p"), oracle_grid(full, 80, 24));
    }

    #[test]
    fn fast_utf8_then_scrolled_ascii_appears_once() {
        // Double-feeding a screenful of unique lines can leave the live
        // grid looking like one pass. The ring must still hold each line
        // once, and the live grid must match one vt100 parse.
        let mut rest = Vec::new();
        for i in 0..40 {
            rest.extend(format!("row{i:02}\r\n").into_bytes());
        }
        let mut full = b"caf\xc3\xa9\r\n".to_vec();
        full.extend_from_slice(&rest);
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        b.feed_output("p", b"caf\xc3\xa9\r\n");
        b.feed_output("p", &rest);
        let tail = b.tail("p", 5000, false).unwrap();
        assert_eq!(tail.iter().filter(|l| *l == "café").count(), 1);
        for i in 0..40 {
            let line = format!("row{i:02}");
            assert_eq!(tail.iter().filter(|l| *l == &line).count(), 1, "{line}");
        }
        assert_eq!(b.snapshot("p"), oracle_contents(&full, 80, 24));
        assert_eq!(surface_screen_texts(&b, "p"), oracle_grid(&full, 80, 24));
    }

    /// Deterministic PRNG (no external crate): the property test below must
    /// be reproducible seed by seed.
    struct FuzzRng(u64);
    impl FuzzRng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// One mixed stream that never wraps or scrolls, so the ring's full
    /// text is exactly the independent parser's visible grid. Cursor
    /// tracking mirrors vt100 for the bytes this emits (CR resets the
    /// column, LF/VT/FF do not, wide characters take their display width).
    fn mixed_utf8_stream(rng: &mut FuzzRng) -> Vec<u8> {
        const COLS: usize = 80;
        // Leave two rows unused so a line-feed never scrolls.
        const MAX_ROW: usize = 22;
        const ASCII_TOKENS: [&str; 8] = ["a", "ok", "Hi!", "line", "xyz", "123", "  ", "-x-"];
        const UTF8_TOKENS: [(&str, usize); 6] = [
            ("é", 1),
            ("✓", 1),
            ("你", 2),
            ("🎉", 2),
            ("café", 4),
            ("über", 4),
        ];
        const ESC_TOKENS: [&str; 7] = [
            "\x1b[31m", "\x1b[32m", "\x1b[1m", "\x1b[0m", "\x1b[K", "\x1b[2K", "\x07",
        ];
        let mut full = Vec::with_capacity(256);
        let mut row = 0usize;
        let mut col = 0usize;
        let down = |full: &mut Vec<u8>, row: &mut usize, col: &mut usize, bare: bool| {
            if *row + 1 >= MAX_ROW {
                full.push(b'\r');
                *col = 0;
                return;
            }
            if bare {
                full.push(b'\n');
            } else {
                full.extend_from_slice(b"\r\n");
                *col = 0;
            }
            *row += 1;
        };
        for _ in 0..32 {
            match rng.below(10) {
                0..=3 => {
                    let t = ASCII_TOKENS[rng.below(ASCII_TOKENS.len() as u64) as usize];
                    if col + t.len() > COLS {
                        down(&mut full, &mut row, &mut col, false);
                    }
                    full.extend_from_slice(t.as_bytes());
                    col += t.len();
                }
                4..=5 => {
                    let (t, w) = UTF8_TOKENS[rng.below(UTF8_TOKENS.len() as u64) as usize];
                    if col + w > COLS {
                        down(&mut full, &mut row, &mut col, false);
                    }
                    full.extend_from_slice(t.as_bytes());
                    col += w;
                }
                6..=8 => match rng.below(6) {
                    0..=2 => down(&mut full, &mut row, &mut col, false),
                    3 => down(&mut full, &mut row, &mut col, true),
                    4 => {
                        // VT and FF are fast bytes and share vt100's `lf()`.
                        if row + 1 >= MAX_ROW {
                            full.push(b'\r');
                            col = 0;
                        } else {
                            full.push(if rng.below(2) == 0 { 0x0b } else { 0x0c });
                            row += 1;
                        }
                    }
                    _ => {
                        full.push(b'\r');
                        col = 0;
                    }
                },
                _ => {
                    let t = ESC_TOKENS[rng.below(ESC_TOKENS.len() as u64) as usize];
                    full.extend_from_slice(t.as_bytes());
                }
            }
        }
        full
    }

    #[test]
    fn fast_ring_matches_independent_oracle_on_mixed_utf8_feeds() {
        // Property-style: a few thousand deterministic streams of ASCII /
        // UTF-8 (é, ✓, CJK, emoji) / CR / LF / VT / FF / ESC (SGR, EL, BEL),
        // each split at random chunk boundaries. Nothing wraps or scrolls,
        // so the ring's full text and the live screen must equal one
        // independent vt100 parse of the concatenated bytes. `snapshot`
        // is the surface parser: a double-feed diverges from the oracle
        // even when the ring was reconciled against that double-feed.
        const CASES: u64 = 3072;
        for case in 0..CASES {
            let mut rng = FuzzRng(case.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(0xD1CE));
            let full = mixed_utf8_stream(&mut rng);
            let mut b = HeadlessBackend::new();
            b.create_surface("p", 80, 24);
            let mut pos = 0;
            while pos < full.len() {
                let n = 1 + rng.below(12) as usize;
                let end = (pos + n).min(full.len());
                b.feed_output("p", &full[pos..end]);
                pos = end;
            }
            let expect_grid = oracle_grid(&full, 80, 24);
            assert_eq!(
                b.snapshot("p"),
                oracle_contents(&full, 80, 24),
                "parser diverged from single-parse oracle (case={case} input={full:?})"
            );
            assert_eq!(
                b.tail("p", 5000, false).unwrap().join("\n"),
                oracle_text(&full, 80, 24),
                "ring text diverged from single-parse oracle (case={case} input={full:?})"
            );
            assert_eq!(
                surface_screen_texts(&b, "p"),
                expect_grid,
                "screen vec diverged from single-parse oracle (case={case} input={full:?})"
            );
        }
    }

    /// Release-mode perf gate for the rendered ring. Ignored by default
    /// (timing-sensitive); run with
    /// `cargo test --release -p signaltty-term -- --ignored fast_ring_perf`.
    /// Bounds are generous (~20x above measured) so this only fires on a
    /// real algorithmic regression back toward per-batch reconciliation.
    #[test]
    #[ignore = "release-only perf gate"]
    fn fast_ring_perf_regression() {
        use std::time::{Duration, Instant};
        let budget = if cfg!(debug_assertions) {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(1)
        };
        let mut bulk = Vec::with_capacity(1024 * 1024);
        let mut i = 0u32;
        while bulk.len() < 1024 * 1024 {
            bulk.extend_from_slice(format!("bulk line {i:08} {:*<60}\r\n", "").as_bytes());
            i += 1;
        }
        bulk.truncate(1024 * 1024);
        let mut b = HeadlessBackend::new();
        b.create_surface("p", 80, 24);
        let t = Instant::now();
        b.feed_output("p", &bulk);
        assert!(
            t.elapsed() < budget,
            "1MiB bulk feed took {:?} (budget {budget:?})",
            t.elapsed()
        );
        assert!(b.ring_len("p") > 0);
    }
}

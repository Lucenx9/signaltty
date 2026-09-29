//! Divider tracking behind one interface: App feeds observations
//! (builds, drags, ticks, server layouts) and executes the returned
//! commands (place a divider, send a ratio). All ratio, pending and
//! echo-suppression state lives here; the module holds no widgets,
//! so the whole settle/echo state machine is unit-testable without
//! GTK — App keeps only the weak widget refs (see `App::paned_widgets`).
//!
//! Protocol with App, per `(tab_id, path)`:
//! - `track` on build, `drop_tab` on rebuild/close.
//! - `position_changed` on every `notify::position` (suppressed moves
//!   are ignored via [`Dividers::suppressing`]).
//! - `tick` with a snapshot of live widgets returns [`Command`]s.
//! - `apply_server` on ratio-only refreshes returns moves; dividers
//!   with unsent drags always win over the echo.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use signaltty_core::model::Layout;

/// Ratios within this of the server value count as unchanged; keeps
/// pixel rounding from scheduling endless sends.
pub const RATIO_EPS: f32 = 0.002;
/// A dragged divider is sent once it rests this long: never mid-drag.
pub const RATIO_SETTLE: Duration = Duration::from_millis(300);

struct Entry {
    /// Last ratio written from server data (build or echo).
    applied: f32,
    /// Set once allocated and positioned, or once the user drags
    /// (user intent wins over the initial placement).
    positioned: bool,
    /// User-dragged ratio awaiting settle + send, with last move time.
    pending: Option<(f32, Instant)>,
}

/// Work for App to do: move a widget, or persist a rested drag.
pub enum Command {
    Place {
        tab_id: String,
        path: Vec<bool>,
        position_px: i32,
    },
    Send {
        tab_id: String,
        path: Vec<bool>,
        ratio: f32,
    },
}

/// Interior mutability throughout (like `App`): observations arrive
/// from synchronous GTK callbacks, so methods take `&self` and the
/// echo-suppression flag never sits under a map borrow.
#[derive(Default)]
pub struct Dividers {
    tabs: RefCell<HashMap<String, HashMap<Vec<bool>, Entry>>>,
    suppress: Cell<bool>,
}

impl Dividers {
    pub fn new() -> Dividers {
        Dividers::default()
    }

    /// Register a divider built from the server layout.
    pub fn track(&self, tab_id: &str, path: Vec<bool>, ratio: f32) {
        self.tabs
            .borrow_mut()
            .entry(tab_id.to_string())
            .or_default()
            .insert(
                path,
                Entry {
                    applied: ratio,
                    positioned: false,
                    pending: None,
                },
            );
    }

    /// Forget a tab (rebuild or close drops its widgets).
    pub fn drop_tab(&self, tab_id: &str) {
        self.tabs.borrow_mut().remove(tab_id);
    }

    /// Run `f` with drag detection off, for programmatic moves
    /// (`notify::position` fires synchronously inside `f`).
    pub fn suppressing<R>(&self, f: impl FnOnce() -> R) -> R {
        self.suppress.set(true);
        let out = f();
        self.suppress.set(false);
        out
    }

    /// A divider moved. Records the ratio for the settle-then-send
    /// pass; dragging back onto the server value clears the queue.
    pub fn position_changed(
        &self,
        tab_id: &str,
        path: &[bool],
        position_px: i32,
        total_px: i32,
        now: Instant,
    ) {
        if self.suppress.get() || total_px <= 0 {
            return;
        }
        let mut tabs = self.tabs.borrow_mut();
        let Some(entry) = tabs.get_mut(tab_id).and_then(|t| t.get_mut(path)) else {
            return;
        };
        let ratio = (position_px as f32 / total_px as f32).clamp(0.0, 1.0);
        entry.positioned = true; // the user owns this divider now
        if (ratio - entry.applied).abs() <= RATIO_EPS {
            entry.pending = None;
        } else {
            entry.pending = Some((ratio, now));
        }
    }

    /// One pass over `live` widgets (`(tab_id, path, total_px)`).
    /// Entries missing from the snapshot are dead and pruned.
    /// Unallocated widgets (`total_px <= 100`) are skipped for now.
    pub fn tick(&self, live: &[(String, Vec<bool>, i32)], now: Instant) -> Vec<Command> {
        let mut commands = Vec::new();
        let mut seen = HashMap::new();
        let tabs = self.tabs.borrow();
        for (tab_id, path, total) in live {
            seen.insert((tab_id.clone(), path.clone()), *total);
            let Some(entry) = tabs.get(tab_id).and_then(|t| t.get(path.as_slice())) else {
                continue;
            };
            if *total <= 100 {
                continue;
            }
            if !entry.positioned {
                commands.push(Command::Place {
                    tab_id: tab_id.clone(),
                    path: path.clone(),
                    position_px: (*total as f32 * entry.applied) as i32,
                });
            }
            if let Some((ratio, at)) = entry.pending {
                if now.duration_since(at) >= RATIO_SETTLE {
                    commands.push(Command::Send {
                        tab_id: tab_id.clone(),
                        path: path.clone(),
                        ratio,
                    });
                }
            }
        }
        drop(tabs);
        self.tabs.borrow_mut().retain(|tab_id, entries| {
            entries.retain(|path, _| seen.contains_key(&(tab_id.clone(), path.clone())));
            !entries.is_empty()
        });
        commands
    }

    /// Mark a divider placed (initial or server-driven move done).
    pub fn placed(&self, tab_id: &str, path: &[bool]) {
        if let Some(entry) = self
            .tabs
            .borrow_mut()
            .get_mut(tab_id)
            .and_then(|t| t.get_mut(path))
        {
            entry.positioned = true;
        }
    }

    /// A send landed; the echoed ratio becomes the baseline (the
    /// server may have clamped it).
    pub fn send_succeeded(&self, tab_id: &str, path: &[bool], confirmed: f32) {
        if let Some(entry) = self
            .tabs
            .borrow_mut()
            .get_mut(tab_id)
            .and_then(|t| t.get_mut(path))
        {
            entry.applied = confirmed;
            entry.pending = None;
        }
    }

    /// A send failed: keep the drag queued for the next tick, unless
    /// its divider is gone, in which case drop it as stale.
    pub fn send_failed(&self, tab_id: &str, path: &[bool], split_alive: bool) {
        if split_alive {
            return;
        }
        if let Some(entry) = self
            .tabs
            .borrow_mut()
            .get_mut(tab_id)
            .and_then(|t| t.get_mut(path))
        {
            entry.pending = None;
        }
    }

    /// Ratio-only server update: move the baseline and report which
    /// dividers App should move. Unpositioned dividers only update
    /// the baseline (the tick places from it); dividers with unsent
    /// drags are left alone — their send carries the newer value.
    pub fn apply_server(&self, tab_id: &str, layout: &Layout) -> Vec<(Vec<bool>, f32)> {
        let mut moves = Vec::new();
        let mut tabs = self.tabs.borrow_mut();
        let Some(entries) = tabs.get_mut(tab_id) else {
            return moves;
        };
        for (path, entry) in entries.iter_mut() {
            if entry.pending.is_some() {
                continue;
            }
            let Some(ratio) = layout.ratio_at_path(path) else {
                continue;
            };
            if (entry.applied - ratio).abs() <= RATIO_EPS {
                continue;
            }
            entry.applied = ratio;
            if entry.positioned {
                moves.push((path.clone(), ratio));
            }
        }
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Instant {
        Instant::now()
    }

    fn live(tab: &str, path: &[bool], total: i32) -> (String, Vec<bool>, i32) {
        (tab.to_string(), path.to_vec(), total)
    }

    #[test]
    fn fresh_divider_places_once_allocated() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        assert!(d.tick(&[live("tab", &[], 50)], now()).is_empty());
        let cmds = d.tick(&[live("tab", &[], 1000)], now());
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Place {
                position_px: 250,
                ..
            }
        ));
        d.placed("tab", &[]);
        assert!(d.tick(&[live("tab", &[], 1000)], now()).is_empty());
    }

    #[test]
    fn drag_sends_only_after_settle() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.placed("tab", &[]);
        let t0 = now();
        d.position_changed("tab", &[], 400, 1000, t0);
        assert!(d.tick(&[live("tab", &[], 1000)], t0).is_empty());
        let cmds = d.tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE);
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Command::Send { ratio, .. } if (ratio - 0.4).abs() < 1e-6));
        d.send_succeeded("tab", &[], 0.4);
        assert!(d
            .tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE * 2)
            .is_empty());
    }

    #[test]
    fn drag_back_to_server_value_clears_queue() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.placed("tab", &[]);
        let t0 = now();
        d.position_changed("tab", &[], 400, 1000, t0);
        d.position_changed("tab", &[], 250, 1000, t0);
        assert!(d
            .tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE)
            .is_empty());
    }

    #[test]
    fn suppressed_moves_are_not_drags() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.placed("tab", &[]);
        let t0 = now();
        d.suppressing(|| d.position_changed("tab", &[], 900, 1000, t0));
        assert!(d
            .tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE)
            .is_empty());
    }

    #[test]
    fn unsent_drag_wins_over_server_echo() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.placed("tab", &[]);
        let t0 = now();
        d.position_changed("tab", &[], 400, 1000, t0);
        // Echo of the old value: skipped, drag intact.
        let layout = serde_json::from_value::<Layout>(serde_json::json!({
            "type": "split", "dir": "right", "ratio": 0.25,
            "first": {"type": "pane", "pane_id": "a"},
            "second": {"type": "pane", "pane_id": "b"},
        }))
        .unwrap();
        assert!(d.apply_server("tab", &layout).is_empty());
        let cmds = d.tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE);
        assert!(matches!(cmds[0], Command::Send { .. }));
    }

    #[test]
    fn failed_send_retries_unless_stale() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.placed("tab", &[]);
        let t0 = now();
        d.position_changed("tab", &[], 400, 1000, t0);
        d.send_failed("tab", &[], true);
        let cmds = d.tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE);
        assert!(matches!(cmds[0], Command::Send { .. }));
        d.send_failed("tab", &[], false);
        assert!(d
            .tick(&[live("tab", &[], 1000)], t0 + RATIO_SETTLE)
            .is_empty());
    }

    #[test]
    fn tick_prunes_dead_widgets() {
        let d = Dividers::new();
        d.track("tab", vec![], 0.25);
        d.track("tab", vec![true], 0.5);
        let cmds = d.tick(&[live("tab", &[], 1000)], now());
        assert!(cmds
            .iter()
            .all(|c| matches!(c, Command::Place { path, .. } if path.is_empty())));
        d.drop_tab("tab");
        assert!(d.tick(&[live("tab", &[], 1000)], now()).is_empty());
    }
}

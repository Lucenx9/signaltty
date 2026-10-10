//! Status vocabulary shared by every surface — sidebar rows, tabs,
//! pane headers, the header "needs you" button — so lifecycle and
//! attention always look and read the same. Colours live in
//! data/style.css under the class names produced here.

use chrono::{DateTime, Utc};
use gtk4::prelude::*;

use signaltty_core::{Attention, Lifecycle, LiveState, Pane};

const ATTENTION_CLASSES: [&str; 5] = [
    "attention-unread",
    "attention-input",
    "attention-permission",
    "attention-warning",
    "attention-error",
];

const LIFECYCLE_CLASSES: [&str; 4] = [
    "lifecycle-working",
    "lifecycle-done",
    "lifecycle-blocked",
    "lifecycle-failed",
];

pub fn attention_class(a: Attention) -> Option<&'static str> {
    match a {
        Attention::None => None,
        Attention::Unread => Some("attention-unread"),
        Attention::InputRequired => Some("attention-input"),
        Attention::PermissionRequired => Some("attention-permission"),
        Attention::Warning => Some("attention-warning"),
        Attention::Error => Some("attention-error"),
    }
}

/// Pill text. Unread is a plain dot, not a pill.
fn attention_label(a: Attention) -> Option<&'static str> {
    match a {
        Attention::InputRequired => Some("Input"),
        Attention::PermissionRequired => Some("Approval"),
        Attention::Warning => Some("Warning"),
        Attention::Error => Some("Error"),
        Attention::None | Attention::Unread => None,
    }
}

pub fn attention_tooltip(a: Attention) -> &'static str {
    match a {
        Attention::None => "",
        Attention::Unread => "New activity",
        Attention::InputRequired => "The agent is asking a question",
        Attention::PermissionRequired => "The agent is waiting for approval",
        Attention::Warning => "Something went wrong, the agent continued",
        Attention::Error => "Failed — needs a look",
    }
}

/// Tab indicator icon (bundled in data/icons).
pub fn attention_icon(a: Attention) -> Option<&'static str> {
    match a {
        Attention::None => None,
        Attention::Unread => Some("signaltty-unread-symbolic"),
        Attention::InputRequired => Some("signaltty-input-symbolic"),
        Attention::PermissionRequired => Some("signaltty-permission-symbolic"),
        Attention::Warning => Some("signaltty-warning-symbolic"),
        Attention::Error => Some("signaltty-error-symbolic"),
    }
}

pub fn lifecycle_label(l: Lifecycle) -> &'static str {
    match l {
        Lifecycle::Unknown => "Running",
        Lifecycle::Working => "Working",
        Lifecycle::Blocked => "Waiting for you",
        Lifecycle::Done => "Done",
        Lifecycle::Idle => "Idle",
        Lifecycle::Failed => "Failed",
        Lifecycle::Exited => "Exited",
    }
}

/// What a sidebar row's status slot says: the one state worth a word
/// and a colour, or `label: None` to show the relative time instead
/// (with an accent dot when `class` is unread).
#[derive(Debug, PartialEq, Eq)]
pub struct RowStatus {
    pub label: Option<&'static str>,
    pub class: Option<&'static str>,
    pub pulse: bool,
}

/// Attention outranks lifecycle; "Done" only speaks while unseen —
/// once looked at, a finished turn is just a time.
pub fn row_status(lifecycle: Lifecycle, attention: Attention) -> RowStatus {
    let say = |label, class| RowStatus {
        label: Some(label),
        class: Some(class),
        pulse: false,
    };
    if let Some(label) = attention_label(attention) {
        return say(label, attention_class(attention).unwrap_or_default());
    }
    match (lifecycle, attention) {
        (Lifecycle::Working, _) => RowStatus {
            pulse: true,
            ..say("Working", "lifecycle-working")
        },
        (Lifecycle::Blocked, _) => say("Waiting", "lifecycle-blocked"),
        (Lifecycle::Failed, _) => say("Failed", "lifecycle-failed"),
        (Lifecycle::Done, Attention::Unread) => say("Done", "lifecycle-done"),
        (_, Attention::Unread) => RowStatus {
            label: None,
            class: Some("attention-unread"),
            pulse: false,
        },
        _ => RowStatus {
            label: None,
            class: None,
            pulse: false,
        },
    }
}

/// Slot text, t3code-style: "Working 3m", "Approval 2m". Under a
/// minute the word stands alone — the 30s tick would leave live
/// seconds stale.
pub fn slot_text(label: Option<&str>, time: &str, age: &str) -> String {
    match label {
        Some(word) if !age.is_empty() && age != "now" => format!("{word} {age}"),
        Some(word) => word.to_string(),
        None => time.to_string(),
    }
}

/// Verb-tense run state (docs/14 §6): "Working for 2m…" while the
/// agent runs, "Worked for 2m" once the turn is done — same slot, no
/// mode switch. `None` when the pane is in neither state or the
/// timing is unknown (snapshots from before timing was recorded).
pub fn run_label(
    lifecycle: Lifecycle,
    since: Option<DateTime<Utc>>,
    last_run_secs: Option<i64>,
    now: DateTime<Utc>,
) -> Option<String> {
    match lifecycle {
        Lifecycle::Working => {
            let secs = (now - since?).num_seconds();
            // The sidebar ticks every 30s; live seconds would read stale.
            Some(if secs < 60 {
                "Working…".to_string()
            } else {
                format!("Working for {}…", duration(secs))
            })
        }
        Lifecycle::Done => Some(format!("Worked for {}", duration(last_run_secs?))),
        _ => None,
    }
}

/// "40s", "2m", "1h 5m".
fn duration(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ if secs % 3600 < 60 => format!("{}h", secs / 3600),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

/// Swap `class` in for whichever of `family` is currently set.
fn set_class(w: &impl IsA<gtk4::Widget>, family: &[&str], class: Option<&str>) {
    for c in family {
        if Some(*c) != class {
            w.remove_css_class(c);
        }
    }
    if let Some(c) = class {
        w.add_css_class(c);
    }
}

pub fn set_attention_class(w: &impl IsA<gtk4::Widget>, a: Attention) {
    set_class(w, &ATTENTION_CLASSES, attention_class(a));
}

/// Worst attention across panes (core severity order).
pub fn worst_attention<'a>(panes: impl IntoIterator<Item = &'a Pane>) -> Attention {
    panes
        .into_iter()
        .fold(Attention::None, |acc, p| acc.raise(p.attention))
}

/// Worst lifecycle across panes (core urgency order).
pub fn worst_lifecycle<'a>(panes: impl IntoIterator<Item = &'a Pane>) -> Lifecycle {
    panes
        .into_iter()
        .map(effective_lifecycle)
        .max_by_key(|l| l.urgency())
        .unwrap_or(Lifecycle::Unknown)
}

/// Saved agent history cannot report activity without a live process.
pub fn effective_lifecycle(pane: &Pane) -> Lifecycle {
    match pane.live {
        LiveState::Live => pane.lifecycle,
        LiveState::Exited { .. } => Lifecycle::Exited,
    }
}

/// Leading lifecycle mark: a dot coloured by state that pulses while
/// working (hollow when there is nothing to report).
pub struct LifecycleIndicator {
    pub widget: gtk4::Box,
    dot: gtk4::Box,
}

impl LifecycleIndicator {
    pub fn new() -> LifecycleIndicator {
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        dot.set_valign(gtk4::Align::Center);
        dot.set_halign(gtk4::Align::Center);
        // Homogeneous: the dot gets the whole 12px cell and centres in it
        // without asking its parent for expansion.
        let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        widget.set_homogeneous(true);
        widget.set_size_request(12, 12);
        widget.set_valign(gtk4::Align::Center);
        widget.append(&dot);
        LifecycleIndicator { widget, dot }
    }

    pub fn set(&self, l: Lifecycle) {
        let class = match l {
            Lifecycle::Working => Some("lifecycle-working"),
            Lifecycle::Done => Some("lifecycle-done"),
            Lifecycle::Blocked => Some("lifecycle-blocked"),
            Lifecycle::Failed => Some("lifecycle-failed"),
            _ => None,
        };
        set_class(&self.dot, &LIFECYCLE_CLASSES, class);
        if matches!(l, Lifecycle::Idle | Lifecycle::Exited) {
            self.dot.add_css_class("hollow");
        } else {
            self.dot.remove_css_class("hollow");
        }
        set_pulse(&self.dot, l == Lifecycle::Working);
        self.widget.set_tooltip_text(Some(lifecycle_label(l)));
    }
}

/// Working is shown by a breathing dot (CSS `pulse`), not a spinner:
/// calmer beside text, and the same mark shape as every other state.
fn set_pulse(dot: &gtk4::Box, on: bool) {
    if on {
        dot.add_css_class("pulse");
    } else {
        dot.remove_css_class("pulse");
    }
}

/// Sidebar status slot: `[mark] label`, coloured by one class on the
/// slot (the mark paints `currentColor`). Shows the relative time when
/// `row_status` has nothing to say.
pub struct StatusSlot {
    pub widget: gtk4::Box,
    mark: gtk4::Box,
    dot: gtk4::Box,
    label: gtk4::Label,
}

const ROW_STATUS_CLASSES: [&str; 9] = [
    "attention-unread",
    "attention-input",
    "attention-permission",
    "attention-warning",
    "attention-error",
    "lifecycle-working",
    "lifecycle-blocked",
    "lifecycle-failed",
    "lifecycle-done",
];

impl StatusSlot {
    pub fn new() -> StatusSlot {
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("row-status-dot");
        dot.set_valign(gtk4::Align::Center);
        dot.set_halign(gtk4::Align::Center);
        let mark = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        mark.set_homogeneous(true);
        mark.set_size_request(12, 12);
        mark.set_valign(gtk4::Align::Center);
        mark.append(&dot);
        let label = gtk4::Label::new(None);
        label.add_css_class("numeric");
        let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        widget.add_css_class("row-status");
        widget.set_halign(gtk4::Align::End);
        widget.set_valign(gtk4::Align::Center);
        widget.append(&mark);
        widget.append(&label);
        StatusSlot {
            widget,
            mark,
            dot,
            label,
        }
    }

    /// `time` fills the slot when the status has no word of its own;
    /// `age` (how long the status has held) follows a word that speaks.
    pub fn set(&self, lifecycle: Lifecycle, attention: Attention, time: &str, age: &str) {
        let s = row_status(lifecycle, attention);
        set_class(&self.widget, &ROW_STATUS_CLASSES, s.class);
        if s.label.is_some() {
            self.widget.add_css_class("speaking");
        } else {
            self.widget.remove_css_class("speaking");
        }
        self.mark.set_visible(s.class.is_some());
        set_pulse(&self.dot, s.pulse);
        let text = slot_text(s.label, time, age);
        if self.label.text() != text {
            self.label.set_text(&text);
        }
        let tooltip = match attention {
            Attention::None => lifecycle_label(lifecycle),
            a => attention_tooltip(a),
        };
        self.widget.set_tooltip_text(Some(tooltip));
    }
}

/// Trailing attention mark: a labelled pill for anything that needs
/// the human, an accent dot for plain unread, nothing otherwise.
/// Fades in and out; never shifts neighbours while fading.
pub struct AttentionBadge {
    pub widget: gtk4::Revealer,
    stack: gtk4::Stack,
    pill: gtk4::Label,
    dot: gtk4::Box,
}

impl AttentionBadge {
    pub fn new() -> AttentionBadge {
        let pill = gtk4::Label::new(None);
        pill.add_css_class("status-pill");
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        dot.add_css_class("attention-unread");
        dot.set_valign(gtk4::Align::Center);
        dot.set_halign(gtk4::Align::End);
        let stack = gtk4::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_interpolate_size(false);
        stack.add_named(&pill, Some("pill"));
        stack.add_named(&dot, Some("dot"));
        let widget = gtk4::Revealer::new();
        widget.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
        widget.set_transition_duration(150);
        widget.set_valign(gtk4::Align::Center);
        widget.set_child(Some(&stack));
        AttentionBadge {
            widget,
            stack,
            pill,
            dot,
        }
    }

    pub fn set(&self, a: Attention) {
        self.widget.set_tooltip_text(Some(attention_tooltip(a)));
        match (a, attention_label(a)) {
            (Attention::None, _) => {
                self.widget.set_reveal_child(false);
                return;
            }
            (_, Some(label)) => {
                self.pill.set_text(label);
                set_attention_class(&self.pill, a);
                self.stack.set_visible_child(&self.pill);
            }
            (_, None) => self.stack.set_visible_child(&self.dot),
        }
        self.widget.set_reveal_child(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_agent_state_does_not_report_a_running_process() {
        let mut pane = Pane::new(
            "ws".into(),
            "tab".into(),
            "/tmp".into(),
            vec!["sh".into()],
            signaltty_core::PtySize::default(),
            Utc::now(),
        );
        for saved in [Lifecycle::Unknown, Lifecycle::Working, Lifecycle::Blocked] {
            pane.lifecycle = saved;
            pane.live = signaltty_core::LiveState::Exited { code: None };
            pane.restore_state = signaltty_core::RestoreState::Restored;
            let displayed = worst_lifecycle([&pane]);
            assert_eq!(lifecycle_label(displayed), "Exited");
            assert!(!row_status(displayed, Attention::None).pulse);
            assert_eq!(pane.lifecycle, saved, "saved history is retained");
        }
        pane.live = signaltty_core::LiveState::Live;
        pane.lifecycle = Lifecycle::Working;
        assert!(row_status(worst_lifecycle([&pane]), Attention::None).pulse);
    }

    #[test]
    fn every_attention_has_a_consistent_presentation() {
        for a in [
            Attention::Unread,
            Attention::InputRequired,
            Attention::PermissionRequired,
            Attention::Warning,
            Attention::Error,
        ] {
            let class = attention_class(a).unwrap();
            assert!(ATTENTION_CLASSES.contains(&class));
            assert!(attention_icon(a).is_some());
            assert!(!attention_tooltip(a).is_empty());
        }
        assert_eq!(attention_class(Attention::None), None);
        assert_eq!(attention_label(Attention::Unread), None);
        assert_eq!(
            attention_label(Attention::PermissionRequired),
            Some("Approval")
        );
    }

    #[test]
    fn row_status_speaks_for_the_loudest_state_only() {
        let s = row_status;
        let label = |l, a| s(l, a).label;
        assert_eq!(
            label(Lifecycle::Working, Attention::PermissionRequired),
            Some("Approval"),
            "a gate outranks progress"
        );
        assert_eq!(label(Lifecycle::Blocked, Attention::None), Some("Waiting"));
        assert_eq!(label(Lifecycle::Failed, Attention::None), Some("Failed"));
        let working = s(Lifecycle::Working, Attention::Unread);
        assert_eq!(working.label, Some("Working"));
        assert!(working.pulse);
        assert_eq!(label(Lifecycle::Done, Attention::Unread), Some("Done"));
        assert_eq!(label(Lifecycle::Done, Attention::None), None, "seen → time");
        assert_eq!(
            s(Lifecycle::Idle, Attention::Unread),
            RowStatus {
                label: None,
                class: Some("attention-unread"),
                pulse: false
            }
        );
        assert_eq!(s(Lifecycle::Idle, Attention::None).class, None);
    }

    #[test]
    fn slot_text_ages_spoken_words_only() {
        assert_eq!(slot_text(Some("Working"), "now", "3m"), "Working 3m");
        assert_eq!(slot_text(Some("Working"), "now", "now"), "Working");
        assert_eq!(slot_text(Some("Approval"), "5m", ""), "Approval");
        assert_eq!(slot_text(None, "5m", "2m"), "5m");
    }

    #[test]
    fn run_label_switches_tense_in_place() {
        let now = Utc::now();
        let ago = |s| Some(now - chrono::Duration::seconds(s));
        let run = |l, since, last| run_label(l, since, last, now);
        assert_eq!(run(Lifecycle::Working, ago(20), None).unwrap(), "Working…");
        assert_eq!(
            run(Lifecycle::Working, ago(150), None).unwrap(),
            "Working for 2m…"
        );
        assert_eq!(
            run(Lifecycle::Done, ago(5), Some(40)).unwrap(),
            "Worked for 40s"
        );
        assert_eq!(
            run(Lifecycle::Done, None, Some(3900)).unwrap(),
            "Worked for 1h 5m"
        );
        assert_eq!(
            run(Lifecycle::Done, None, Some(7210)).unwrap(),
            "Worked for 2h"
        );
        assert_eq!(run(Lifecycle::Working, None, None), None);
        assert_eq!(run(Lifecycle::Done, ago(5), None), None);
        assert_eq!(run(Lifecycle::Idle, ago(5), Some(40)), None);
    }
}

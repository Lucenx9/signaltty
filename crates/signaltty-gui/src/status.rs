//! Status vocabulary shared by every surface — sidebar rows, tabs,
//! pane headers, the header "needs you" button — so lifecycle and
//! attention always look and read the same. Colours live in
//! data/style.css under the class names produced here.

use gtk4::prelude::*;

use signaltty_core::{Attention, Lifecycle, Pane};

const ATTENTION_CLASSES: [&str; 5] = [
    "attention-unread",
    "attention-input",
    "attention-permission",
    "attention-warning",
    "attention-error",
];

const LIFECYCLE_CLASSES: [&str; 3] = ["lifecycle-done", "lifecycle-blocked", "lifecycle-failed"];

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
        .map(|p| p.lifecycle)
        .max_by_key(|l| l.urgency())
        .unwrap_or(Lifecycle::Unknown)
}

/// Leading lifecycle mark: a spinner while working, otherwise a dot
/// coloured by state (hollow when there is nothing to report).
pub struct LifecycleIndicator {
    pub widget: gtk4::Stack,
    dot: gtk4::Box,
}

impl LifecycleIndicator {
    pub fn new() -> LifecycleIndicator {
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        dot.set_valign(gtk4::Align::Center);
        dot.set_halign(gtk4::Align::Center);
        let spinner = libadwaita::Spinner::new();
        spinner.add_css_class("status-spinner");
        let widget = gtk4::Stack::new();
        widget.set_transition_type(gtk4::StackTransitionType::Crossfade);
        widget.set_transition_duration(150);
        widget.set_size_request(12, 12);
        widget.set_valign(gtk4::Align::Center);
        widget.add_named(&dot, Some("dot"));
        widget.add_named(&spinner, Some("spinner"));
        LifecycleIndicator { widget, dot }
    }

    pub fn set(&self, l: Lifecycle) {
        let class = match l {
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
        self.widget
            .set_visible_child_name(if l == Lifecycle::Working {
                "spinner"
            } else {
                "dot"
            });
        self.widget.set_tooltip_text(Some(lifecycle_label(l)));
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
}

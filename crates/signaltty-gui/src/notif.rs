//! Desktop notifications for attention. Skipped when the target pane
//! is already focused; clicking focuses the workspace + pane.
//! "Mark read" clears the attention without leaving the current context.

use crate::actor::{UiEvent, UiTx};

/// Map a desktop-notification action to its UI event. Pure seam:
/// headless-tested, no daemon involved.
pub fn action_event(action: &str, pane_id: &str) -> Option<UiEvent> {
    match action {
        "focus" | "default" => Some(UiEvent::FocusPane(pane_id.to_string())),
        "mark-read" => Some(UiEvent::MarkSeen(pane_id.to_string())),
        _ => None,
    }
}

#[derive(Clone)]
pub struct Notifier {
    ui: UiTx,
}

impl Notifier {
    pub fn new(ui: UiTx) -> Notifier {
        Notifier { ui }
    }

    pub fn notify_attention(&self, title: &str, body: &str, pane_id: &str) {
        // Configurable kill-switch: SIGNALTTY_NOTIFY=0 disables popups.
        if std::env::var("SIGNALTTY_NOTIFY").as_deref() == Ok("0") {
            return;
        }
        let mut n = notify_rust::Notification::new();
        n.appname("signaltty")
            .icon("dev.signaltty.gui")
            .hint(notify_rust::Hint::DesktopEntry("dev.signaltty.gui".into()))
            .summary(title)
            .body(body)
            .action("focus", "Focus")
            .action("mark-read", "Mark read")
            .timeout(notify_rust::Timeout::Milliseconds(8000));
        let pane_id = pane_id.to_string();
        match n.show() {
            Ok(handle) => {
                let ui = self.ui.clone();
                std::thread::Builder::new()
                    .name("signaltty-notif".to_string())
                    .spawn(move || {
                        handle.wait_for_action(|action| {
                            if let Some(ev) = action_event(action, &pane_id) {
                                let _ = ui.send(ev);
                            }
                        });
                    })
                    .ok();
            }
            Err(e) => {
                eprintln!("signaltty-gui: notification failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_events_and_unknown_maps_to_none() {
        assert!(matches!(action_event("focus", "p"), Some(UiEvent::FocusPane(p)) if p == "p"));
        assert!(matches!(
            action_event("default", "p"),
            Some(UiEvent::FocusPane(_))
        ));
        assert!(matches!(action_event("mark-read", "p"), Some(UiEvent::MarkSeen(p)) if p == "p"));
        assert!(action_event("snooze", "p").is_none());
    }
}

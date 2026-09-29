//! Desktop notifications for attention. Skipped when the target pane
//! is already focused; clicking focuses the workspace + pane.

use crate::actor::{UiEvent, UiTx};

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
            .timeout(notify_rust::Timeout::Milliseconds(8000));
        let pane_id = pane_id.to_string();
        match n.show() {
            Ok(handle) => {
                let ui = self.ui.clone();
                std::thread::Builder::new()
                    .name("signaltty-notif".to_string())
                    .spawn(move || {
                        handle.wait_for_action(|action| {
                            if action == "focus" || action == "default" {
                                let _ = ui.send(UiEvent::FocusPane(pane_id));
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

//! signaltty-gui: native GTK4/libadwaita client for the session server.
//! The GUI owns no processes and no PTYs; closing it never kills sessions.

mod actions;
mod actor;
mod app;
mod board;
mod changes;
mod details;
mod dividers;
mod metrics;
mod new_workspace;
mod notif;
mod palette;
mod preferences;
mod refresh;
mod sidebar;
mod status;
mod task_chip;
mod terminal;
mod util;
mod workspace_dialogs;

use gtk4::prelude::*;

/// Extract `--socket PATH`, returning the socket plus argv with our
/// flag stripped (GTK rejects unknown options in run()).
fn socket_arg() -> (Option<std::path::PathBuf>, Vec<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut socket = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            socket = Some(std::path::PathBuf::from(&args[i + 1]));
            i += 2;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (socket, rest)
}

fn main() {
    // Before the Application exists: AdwApplication loads style.css and
    // the bundled icons from this resource path.
    gtk4::gio::resources_register_include!("signaltty-gui.gresource")
        .expect("register bundled resources");
    let (socket_arg, gtk_args) = socket_arg();
    let socket = socket_arg.unwrap_or_else(signaltty_core::paths::socket_path);
    let application = libadwaita::Application::new(
        Some("dev.signaltty.gui"),
        gtk4::gio::ApplicationFlags::FLAGS_NONE,
    );
    application.connect_activate(move |application| {
        if let Some(window) = application.active_window() {
            window.present();
            return;
        }
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();
        let actor = actor::spawn(socket.clone(), ui_tx.clone());
        let gui = app::App::new(application, actor, ui_tx);
        let gui_events = gui.clone();
        gtk4::glib::MainContext::default().spawn_local(async move {
            while let Some(ev) = ui_rx.recv().await {
                gui_events.on_event(ev);
            }
        });
        gui.refresh();
        gui.present();
    });
    let _ = application.run_with_args(&gtk_args);
}

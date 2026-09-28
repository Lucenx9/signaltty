//! signaltty-gui: native GTK4/libadwaita client for the session server.
//! The GUI owns no processes and no PTYs; closing it never kills sessions.

mod actor;
mod app;
mod notif;
mod sidebar;
mod terminal;
mod util;

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
    let (socket_arg, gtk_args) = socket_arg();
    let socket = socket_arg.unwrap_or_else(signaltty_core::paths::socket_path);
    let application = libadwaita::Application::new(
        Some("dev.signaltty.gui"),
        gtk4::gio::ApplicationFlags::FLAGS_NONE,
    );
    application.connect_activate(move |application| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(crate::util::CSS);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
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

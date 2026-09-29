//! "New Workspace" dialog: folder, name and first command.
//!
//! The dialog is an [`adw::AlertDialog`] so Enter confirms, Esc
//! cancels and the primary response gets the suggested style for
//! free. Rows are [`adw::ActionRow`]/[`adw::EntryRow`]/[`adw::ComboRow`];
//! the folder picker is [`gtk4::FileDialog`] in folder mode.
//!
//! Agent choices come from the adapter registry
//! (`signaltty_agent::adapter_for_kind`): only agents whose binary is
//! on `PATH` are offered, so the GUI and the server's argv detection
//! can never disagree. Pure helpers (PATH lookup, argv splitting,
//! defaults) are toolkit-free and unit-tested below.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use signaltty_core::model::AgentKind;

use crate::util::tilde;

const RESPONSE_CREATE: &str = "create";
const RESPONSE_CANCEL: &str = "cancel";
const CUSTOM_LABEL: &str = "Custom…";

/// What the dialog hands back when the user confirms.
pub struct NewWorkspaceRequest {
    pub name: String,
    pub cwd: String,
    pub argv: Vec<String>,
}

/// One runnable choice in the command row: a label plus the argv to
/// spawn (`pane.spawn` resolves the binary on `PATH`, like the CLI).
pub struct CommandOption {
    pub label: &'static str,
    pub argv: Vec<String>,
}

/// Agents in dialog order. Binaries come from each adapter's
/// metadata; labels are the short product names.
fn agent_kinds() -> [(AgentKind, &'static str); 4] {
    [
        (AgentKind::Claude, "Claude"),
        (AgentKind::Codex, "Codex"),
        (AgentKind::Opencode, "opencode"),
        (AgentKind::Cursor, "Cursor"),
    ]
}

/// Shell first (always available), then every agent with a binary on
/// `PATH`. Adapters owning several binaries (Cursor) use the first
/// one found.
pub fn available_commands() -> Vec<CommandOption> {
    let mut out = vec![CommandOption {
        label: "Shell",
        argv: vec![crate::app::user_shell()],
    }];
    for (kind, label) in agent_kinds() {
        let meta = signaltty_agent::adapter_for_kind(kind).metadata();
        if let Some(bin) = meta.binaries.iter().find(|b| binary_in_path(b).is_some()) {
            out.push(CommandOption {
                label,
                argv: vec![bin.to_string()],
            });
        }
    }
    out
}

/// Starting an agent is the product's central action, so the first
/// installed agent is preselected; Shell when none is installed.
pub fn default_command_index(commands: &[CommandOption]) -> usize {
    if commands.len() > 1 {
        1
    } else {
        0
    }
}

/// Resolve `name` on `PATH` (bare names only). The executable bit is
/// checked so stale non-executable files don't offer dead commands.
pub fn binary_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    binary_in_path_var(name, &path, is_executable_file)
}

fn binary_in_path_var(
    name: &str,
    path_var: &OsStr,
    is_exec: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') {
        return None;
    }
    std::env::split_paths(path_var)
        .map(|dir| dir.join(name))
        .find(|p| is_exec(p))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Last path component, for the prefilled workspace name.
pub fn folder_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("workspace")
        .to_string()
}

/// Active workspace's folder when there is one, else the home
/// directory (matching `signaltty new --cwd` semantics).
pub fn default_cwd(active_ws_cwd: Option<&str>) -> String {
    if let Some(cwd) = active_ws_cwd.filter(|c| !c.is_empty()) {
        return cwd.to_string();
    }
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "/".to_string())
}

/// Split a custom command shell-style: whitespace separates unless
/// quoted, backslash escapes the next character (literal inside
/// single quotes). Returns `None` on unbalanced quotes.
pub fn split_argv(input: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut in_word = false;
    for c in input.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            in_word = true;
            continue;
        }
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            Some('"') => {
                if c == '"' {
                    quote = None;
                } else if c == '\\' {
                    escaped = true;
                } else {
                    cur.push(c);
                }
            }
            Some(_) => unreachable!("only quote chars are stored"),
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                    in_word = true;
                } else if c == '\\' {
                    escaped = true;
                    in_word = true;
                } else if c.is_whitespace() {
                    if in_word {
                        args.push(std::mem::take(&mut cur));
                        in_word = false;
                    }
                } else {
                    cur.push(c);
                    in_word = true;
                }
            }
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    if in_word {
        args.push(cur);
    }
    Some(args)
}

struct State {
    cwd: String,
    /// The name only follows folder picks until the user edits it.
    name_edited: bool,
    /// Set while the folder picker writes the name, so the change
    /// handler doesn't mistake it for a manual edit.
    setting_name: bool,
}

/// Show the dialog. `on_create` runs once per confirmation with the
/// validated request; `on_close` runs whenever the dialog goes away
/// (used to re-arm the action). Returns the dialog for tests.
pub fn show_dialog(
    parent: &adw::ApplicationWindow,
    active_ws_cwd: Option<&str>,
    on_create: impl Fn(NewWorkspaceRequest) + 'static,
    on_close: impl Fn() + 'static,
) -> adw::AlertDialog {
    let commands = Rc::new(available_commands());
    let default_index = default_command_index(&commands);
    let state = Rc::new(RefCell::new(State {
        cwd: default_cwd(active_ws_cwd),
        name_edited: false,
        setting_name: false,
    }));

    let dialog = adw::AlertDialog::builder()
        .heading("New Workspace")
        .content_width(440)
        .build();
    dialog.add_response(RESPONSE_CANCEL, "Cancel");
    dialog.add_response(RESPONSE_CREATE, "Create");
    dialog.set_response_appearance(RESPONSE_CREATE, adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some(RESPONSE_CREATE));
    dialog.set_close_response(RESPONSE_CANCEL);

    // ---- workspace group ----
    let folder_row = adw::ActionRow::builder()
        .title("Folder")
        .subtitle(tilde(&state.borrow().cwd))
        .activatable(true)
        .build();
    folder_row.set_tooltip_text(Some("The project folder the first command runs in"));
    let browse = gtk4::Button::from_icon_name("folder-open-symbolic");
    browse.set_tooltip_text(Some("Choose a folder"));
    browse.add_css_class("flat");
    browse.set_valign(gtk4::Align::Center);
    folder_row.add_suffix(&browse);

    let name_row = adw::EntryRow::builder()
        .title("Name")
        .text(folder_basename(&state.borrow().cwd))
        .activates_default(true)
        .build();
    dialog.set_focus(Some(&name_row));

    let ws_group = adw::PreferencesGroup::builder().title("Workspace").build();
    ws_group.add(&folder_row);
    ws_group.add(&name_row);

    // ---- command group ----
    let labels: Vec<String> = commands
        .iter()
        .map(|c| c.label.to_string())
        .chain(std::iter::once(CUSTOM_LABEL.to_string()))
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let combo = adw::ComboRow::builder()
        .title("Command")
        .model(&gtk4::StringList::new(&refs))
        .selected(default_index as u32)
        .build();
    combo.set_subtitle(&commands[default_index].argv.join(" "));
    // Only installed agents are listed, so no per-row availability marks.
    combo.set_tooltip_text(Some("The first command to run in the new workspace"));

    let custom_row = adw::EntryRow::builder()
        .title("Custom command")
        .activates_default(true)
        .build();
    custom_row.set_tooltip_text(Some("Example: claude --model opus"));
    custom_row.set_visible(false);

    let cmd_group = adw::PreferencesGroup::builder().title("Command").build();
    cmd_group.add(&combo);
    cmd_group.add(&custom_row);

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    body.append(&ws_group);
    body.append(&cmd_group);
    dialog.set_extra_child(Some(&body));

    // ---- validation ----
    let revalidate = {
        let dialog = dialog.clone();
        let name_row = name_row.clone();
        let custom_row = custom_row.clone();
        let combo = combo.clone();
        let state = Rc::clone(&state);
        let commands = Rc::clone(&commands);
        move || {
            let name_ok = !name_row.text().trim().is_empty();
            let cwd_ok = Path::new(state.borrow().cwd.as_str()).is_dir();
            let sel = combo.selected() as usize;
            let cmd_ok = if sel >= commands.len() {
                split_argv(custom_row.text().trim()).is_some_and(|v| !v.is_empty())
            } else {
                true
            };
            dialog.set_response_enabled(RESPONSE_CREATE, name_ok && cwd_ok && cmd_ok);
        }
    };
    let revalidate = Rc::new(revalidate);
    revalidate();

    // ---- confirm ----
    let on_create = Rc::new(on_create);
    let confirm = {
        let dialog = dialog.clone();
        let name_row = name_row.clone();
        let custom_row = custom_row.clone();
        let combo = combo.clone();
        let state = Rc::clone(&state);
        let commands = Rc::clone(&commands);
        move || {
            let sel = combo.selected() as usize;
            let argv = if sel >= commands.len() {
                split_argv(custom_row.text().trim()).unwrap_or_default()
            } else {
                commands[sel].argv.clone()
            };
            let name = name_row.text().trim().to_string();
            let cwd = state.borrow().cwd.clone();
            if name.is_empty() || argv.is_empty() || !Path::new(&cwd).is_dir() {
                return;
            }
            // Choosing a response already dismisses the dialog; the
            // explicit close only covers entry-activated confirms.
            dialog.close();
            on_create(NewWorkspaceRequest { name, cwd, argv });
        }
    };
    let confirm = Rc::new(confirm);

    {
        let confirm = Rc::clone(&confirm);
        dialog.connect_response(Some(RESPONSE_CREATE), move |_, _| {
            confirm();
        });
    }
    // EntryRow consumes Enter itself; forward it to the same path so
    // Invio always confirms (Esc is handled by close_response).
    for row in [&name_row, &custom_row] {
        let confirm = Rc::clone(&confirm);
        row.connect_entry_activated(move |_| confirm());
    }

    // ---- wiring ----
    {
        let state = Rc::clone(&state);
        let revalidate = Rc::clone(&revalidate);
        name_row.connect_changed(move |_| {
            if !state.borrow().setting_name {
                state.borrow_mut().name_edited = true;
            }
            revalidate();
        });
    }
    {
        let revalidate = Rc::clone(&revalidate);
        custom_row.connect_changed(move |_| revalidate());
    }
    {
        let custom_row = custom_row.clone();
        let commands = Rc::clone(&commands);
        let revalidate = Rc::clone(&revalidate);
        combo.connect_selected_notify(move |combo| {
            let sel = combo.selected() as usize;
            if sel >= commands.len() {
                combo.set_subtitle("Type any command below");
                // Plain visibility, no revealer: nothing animates on
                // keyboard navigation.
                custom_row.set_visible(true);
            } else {
                combo.set_subtitle(&commands[sel].argv.join(" "));
                custom_row.set_visible(false);
            }
            revalidate();
        });
    }
    let browse_action = {
        let parent = parent.clone();
        let folder_row = folder_row.clone();
        let name_row = name_row.clone();
        let state = Rc::clone(&state);
        let revalidate = Rc::clone(&revalidate);
        move || {
            let initial = gio::File::for_path(state.borrow().cwd.clone());
            let picker = gtk4::FileDialog::builder()
                .title("Choose a Project Folder")
                .initial_folder(&initial)
                .build();
            let folder_row = folder_row.clone();
            let name_row = name_row.clone();
            let state = Rc::clone(&state);
            let revalidate = Rc::clone(&revalidate);
            picker.select_folder(Some(&parent), None::<&gio::Cancellable>, move |result| {
                let Ok(folder) = result else {
                    return; // dismissed
                };
                let Some(path) = folder.path().map(|p| p.to_string_lossy().to_string()) else {
                    return;
                };
                state.borrow_mut().cwd = path.clone();
                folder_row.set_subtitle(&tilde(&path));
                if !state.borrow().name_edited {
                    state.borrow_mut().setting_name = true;
                    name_row.set_text(&folder_basename(&path));
                    state.borrow_mut().setting_name = false;
                }
                revalidate();
            });
        }
    };
    let browse_action = Rc::new(browse_action);
    {
        let browse_action = Rc::clone(&browse_action);
        folder_row.connect_activated(move |_| browse_action());
    }
    {
        let browse_action = Rc::clone(&browse_action);
        browse.connect_clicked(move |_| browse_action());
    }

    dialog.connect_closed(move |_| on_close());
    dialog.present(Some(parent));
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::glib;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn basename_falls_back_for_root_and_empty() {
        assert_eq!(folder_basename("/home/u/project"), "project");
        assert_eq!(folder_basename("/home/u/trailing/"), "trailing");
        assert_eq!(folder_basename("/"), "workspace");
        assert_eq!(folder_basename(""), "workspace");
    }

    #[test]
    fn default_cwd_prefers_active_workspace_then_home() {
        assert_eq!(default_cwd(Some("/tmp/x")), "/tmp/x");
        let home = std::env::var("HOME").unwrap();
        assert_eq!(default_cwd(None), home);
        assert_eq!(default_cwd(Some("")), home);
    }

    #[test]
    fn default_command_prefers_first_agent() {
        let shell = || CommandOption {
            label: "Shell",
            argv: vec!["sh".to_string()],
        };
        assert_eq!(default_command_index(&[shell()]), 0);
        assert_eq!(
            default_command_index(&[
                shell(),
                CommandOption {
                    label: "Claude",
                    argv: vec!["claude".to_string()]
                }
            ]),
            1
        );
    }

    #[test]
    fn registry_binaries_are_bare_names() {
        // The PATH lookup only handles bare names; a registry entry
        // with a slash would silently never match.
        for (kind, _) in agent_kinds() {
            for bin in signaltty_agent::adapter_for_kind(kind).metadata().binaries {
                assert!(!bin.is_empty() && !bin.contains('/'), "{bin}");
            }
        }
    }

    #[test]
    fn path_lookup_finds_executables_only() {
        let dir =
            std::env::temp_dir().join(format!("signaltty-dialog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fake-agent");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = dir.join("not-executable");
        std::fs::write(&plain, "x").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        let var = dir.to_string_lossy().to_string();

        assert_eq!(
            binary_in_path_var("fake-agent", OsStr::new(&var), is_executable_file),
            Some(exe)
        );
        #[cfg(unix)]
        assert_eq!(
            binary_in_path_var("not-executable", OsStr::new(&var), is_executable_file),
            None
        );
        assert_eq!(
            binary_in_path_var("missing", OsStr::new(&var), is_executable_file),
            None
        );
        assert_eq!(
            binary_in_path_var("a/b", OsStr::new(&var), is_executable_file),
            None
        );
        assert_eq!(
            binary_in_path_var("", OsStr::new(&var), is_executable_file),
            None
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn split_argv_handles_quotes_and_escapes() {
        let v = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            split_argv("claude --model opus").unwrap(),
            v(&["claude", "--model", "opus"])
        );
        assert_eq!(
            split_argv("  claude   --prompt 'hello world' ").unwrap(),
            v(&["claude", "--prompt", "hello world"])
        );
        assert_eq!(
            split_argv("sh -c \"echo hi\"").unwrap(),
            v(&["sh", "-c", "echo hi"])
        );
        assert_eq!(split_argv("echo a\\ b").unwrap(), v(&["echo", "a b"]));
        assert_eq!(split_argv("echo 'a\\b'").unwrap(), v(&["echo", "a\\b"]));
        assert_eq!(split_argv("").unwrap(), Vec::<String>::new());
        assert_eq!(split_argv("   ").unwrap(), Vec::<String>::new());
        assert!(split_argv("claude 'oops").is_none());
        assert!(split_argv("claude \"oops").is_none());
        assert!(split_argv("claude \\").is_none());
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session (or xvfb-run)"]
    fn dialog_confirm_emits_validated_request() {
        use std::cell::Cell;

        adw::init().unwrap();
        let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();

        let dir = std::env::temp_dir().join("signaltty-dialog-e2e");
        std::fs::create_dir_all(&dir).unwrap();
        let dir_s = dir.to_string_lossy().to_string();

        type Captured = Rc<RefCell<Option<(String, String, Vec<String>)>>>;
        let got: Captured = Rc::new(RefCell::new(None));
        let got2 = Rc::clone(&got);
        let closed = Rc::new(Cell::new(false));
        let closed2 = Rc::clone(&closed);
        let dialog = show_dialog(
            &window,
            Some(&dir_s),
            move |req| {
                *got2.borrow_mut() = Some((req.name, req.cwd, req.argv));
            },
            move || closed2.set(true),
        );
        // Same emission as pressing Enter on the default response.
        dialog.emit_by_name_with_details::<()>(
            "response",
            glib::Quark::from_str(RESPONSE_CREATE),
            &[&RESPONSE_CREATE],
        );
        while glib::MainContext::default().iteration(false) {}

        let commands = available_commands();
        let expected_argv = commands[default_command_index(&commands)].argv.clone();
        assert_eq!(
            got.borrow().clone(),
            Some(("signaltty-dialog-e2e".to_string(), dir_s, expected_argv))
        );
        assert!(closed.get());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

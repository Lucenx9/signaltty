//! New Workspace action dialog: editable draft, validation and async retry.
//! App owns IPC; the form stays mounted until creation succeeds.

use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use signaltty_core::model::AgentKind;

use crate::util::tilde;

const CUSTOM_LABEL: &str = "Custom…";

/// What the dialog hands back when the user confirms.
pub struct NewWorkspaceRequest {
    pub existing_workspace_id: Option<String>,
    pub auto_name: bool,
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
fn agent_kinds() -> [(AgentKind, &'static str); 5] {
    [
        (AgentKind::Claude, "Claude"),
        (AgentKind::Codex, "Codex"),
        (AgentKind::Opencode, "opencode"),
        (AgentKind::Cursor, "Cursor"),
        (AgentKind::Pi, "Pi"),
    ]
}

/// Shell first (always available), then every agent with a binary on
/// `PATH`: built-in adapters, then the generic agents with bundled
/// screen rules. Agents owning several binaries (Cursor) use the
/// first one found.
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
    // Generic agents signaltty has screen rules for (gemini, copilot, …).
    for (label, bins) in signaltty_agent::bundled::bundled_programs() {
        if let Some(bin) = bins.into_iter().find(|b| binary_in_path(b).is_some()) {
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
fn supports_auto_name(argv: &[String]) -> bool {
    argv.first()
        .and_then(|s| Path::new(s).file_name())
        .and_then(|s| s.to_str())
        .is_some_and(|s| matches!(s, "codex" | "claude"))
}

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

pub struct CreationError {
    pub message: String,
    pub workspace_id: Option<String>,
}

pub struct NewWorkspaceDialog {
    pub dialog: adw::Dialog,
    pending: Cell<bool>,
    existing_workspace_id: RefCell<Option<String>>,
    workspace_fields: gtk4::Box,
    command_fields: adw::PreferencesGroup,
    cancel: gtk4::Button,
    create: gtk4::Button,
    spinner: adw::Spinner,
    feedback: gtk4::Label,
    folder_row: adw::ActionRow,
    folder_error: gtk4::Label,
    name_row: adw::EntryRow,
    name_error: gtk4::Label,
    auto_name: adw::SwitchRow,
    combo: adw::ComboRow,
    custom_row: adw::EntryRow,
    command_error: gtk4::Label,
    state: RefCell<State>,
    commands: Vec<CommandOption>,
}

fn error_label() -> gtk4::Label {
    let label = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .build();
    label.add_css_class("error");
    label.set_margin_start(12);
    label.set_margin_end(12);
    label
}

fn field_error(row: &impl IsA<gtk4::Widget>, label: &gtk4::Label, message: &str) {
    label.set_text(message);
    label.set_visible(!message.is_empty());
    if message.is_empty() {
        row.remove_css_class("error");
    } else {
        row.add_css_class("error");
    }
    row.as_ref()
        .update_property(&[gtk4::accessible::Property::Description(message)]);
}

impl NewWorkspaceDialog {
    fn validate(&self) -> bool {
        let folder_error = if Path::new(&self.state.borrow().cwd).is_dir() {
            ""
        } else {
            "Choose an existing project folder."
        };
        let name_error = if self.auto_name.is_active() || !self.name_row.text().trim().is_empty() {
            ""
        } else {
            "Enter a workspace name."
        };
        let command_error = if self.combo.selected() as usize >= self.commands.len() {
            match split_argv(self.custom_row.text().trim()) {
                Some(argv) if !argv.is_empty() && !argv[0].is_empty() => "",
                Some(_) => "Enter a command to start the terminal.",
                None => "Close the quoted text or remove the trailing backslash.",
            }
        } else {
            ""
        };
        field_error(&self.folder_row, &self.folder_error, folder_error);
        field_error(&self.name_row, &self.name_error, name_error);
        field_error(&self.custom_row, &self.command_error, command_error);
        let valid = folder_error.is_empty() && name_error.is_empty() && command_error.is_empty();
        self.create.set_sensitive(valid && !self.pending.get());
        valid
    }

    fn request(&self) -> Option<NewWorkspaceRequest> {
        if self.pending.get() || !self.validate() {
            return None;
        }
        let sel = self.combo.selected() as usize;
        let argv = if sel >= self.commands.len() {
            split_argv(self.custom_row.text().trim())?
        } else {
            self.commands[sel].argv.clone()
        };
        let automatic = self.auto_name.is_active();
        let request = NewWorkspaceRequest {
            existing_workspace_id: self.existing_workspace_id.borrow().clone(),
            name: if automatic {
                folder_basename(&self.state.borrow().cwd)
            } else {
                self.name_row.text().trim().to_string()
            },
            cwd: self.state.borrow().cwd.clone(),
            argv,
            auto_name: automatic,
        };
        self.pending.set(true);
        self.dialog.set_can_close(false);
        self.workspace_fields.set_sensitive(false);
        self.command_fields.set_sensitive(false);
        self.cancel.set_sensitive(false);
        self.create.set_sensitive(false);
        self.create.set_label("Creating…");
        self.spinner.set_visible(true);
        self.feedback.remove_css_class("error");
        self.feedback
            .set_text(if request.existing_workspace_id.is_some() {
                "Starting the command in the existing workspace…"
            } else {
                "Creating the workspace and starting its first command…"
            });
        self.feedback.set_visible(true);
        Some(request)
    }

    pub fn finish(&self, result: Result<(), CreationError>) {
        if !self.pending.replace(false) {
            return;
        }
        self.dialog.set_can_close(true);
        self.spinner.set_visible(false);
        match result {
            Ok(()) => {
                self.dialog.close();
            }
            Err(error) => {
                self.existing_workspace_id.replace(error.workspace_id);
                self.workspace_fields
                    .set_sensitive(self.existing_workspace_id.borrow().is_none());
                self.command_fields.set_sensitive(true);
                self.cancel.set_sensitive(true);
                self.create.set_label("Retry");
                self.feedback.add_css_class("error");
                self.feedback.set_text(&error.message);
                self.validate();
                self.create.grab_focus();
            }
        }
    }
}

/// Keep this form alive until its closed callback fires. Submission starts one
/// attempt; the owner reports its result with `finish`, including partial creation.
pub fn show_dialog(
    parent: &adw::ApplicationWindow,
    active_ws_cwd: Option<&str>,
    automatic_agent_permissions: bool,
    on_create: impl Fn(NewWorkspaceRequest, Rc<NewWorkspaceDialog>) + 'static,
    on_close: impl Fn() + 'static,
) -> Rc<NewWorkspaceDialog> {
    let mut commands = available_commands();
    if automatic_agent_permissions {
        for command in &mut commands {
            command.argv = signaltty_agent::execution::automatic_launch_argv(&command.argv);
        }
    }
    let default_index = default_command_index(&commands);
    let cwd = default_cwd(active_ws_cwd);
    let dialog = adw::Dialog::builder()
        .title("New Workspace")
        .content_width(440)
        .follows_content_size(true)
        .build();
    dialog.add_css_class("signaltty-dialog");
    let cancel = gtk4::Button::with_label("Cancel");
    let create = gtk4::Button::with_label("Create");
    create.add_css_class("suggested-action");
    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel);
    header.pack_end(&create);
    let folder_row = adw::ActionRow::builder()
        .title("Folder")
        .subtitle(tilde(&cwd))
        .use_markup(false)
        .subtitle_lines(2)
        .activatable(true)
        .build();
    let browse = gtk4::Button::from_icon_name("folder-open-symbolic");
    browse.set_tooltip_text(Some("Choose a folder"));
    browse.update_property(&[gtk4::accessible::Property::Label("Choose a project folder")]);
    browse.add_css_class("flat");
    browse.set_valign(gtk4::Align::Center);
    folder_row.add_suffix(&browse);
    let name_row = adw::EntryRow::builder()
        .title("Name")
        .text(folder_basename(&cwd))
        .activates_default(true)
        .build();
    let automatic = supports_auto_name(&commands[default_index].argv);
    let auto_name = adw::SwitchRow::builder()
        .title("Automatic name")
        .subtitle(if automatic {
            "From your first message"
        } else {
            "Available with Codex or Claude"
        })
        .active(automatic)
        .sensitive(automatic)
        .visible(automatic)
        .build();
    auto_name.set_tooltip_text(Some(
        "Uses your Codex or Claude account and its default model; generation may use account quota",
    ));
    name_row.set_visible(!automatic);
    let folder_group = adw::PreferencesGroup::builder().title("Workspace").build();
    folder_group.add(&folder_row);
    let name_group = adw::PreferencesGroup::new();
    name_group.add(&auto_name);
    name_group.add(&name_row);
    let folder_error = error_label();
    let name_error = error_label();
    let workspace_fields = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    workspace_fields.append(&folder_group);
    workspace_fields.append(&folder_error);
    workspace_fields.append(&name_group);
    workspace_fields.append(&name_error);

    let labels: Vec<&str> = commands
        .iter()
        .map(|command| command.label)
        .chain(std::iter::once(CUSTOM_LABEL))
        .collect();
    let combo = adw::ComboRow::builder()
        .title("Command")
        .use_markup(false)
        .subtitle_lines(2)
        .model(&gtk4::StringList::new(&labels))
        .selected(default_index as u32)
        .build();
    combo.set_subtitle(&commands[default_index].argv.join(" "));
    let custom_row = adw::EntryRow::builder()
        .title("Custom command")
        .activates_default(true)
        .visible(false)
        .build();
    custom_row.set_tooltip_text(Some("Example: claude --model opus"));
    let command_fields = adw::PreferencesGroup::builder().title("Command").build();
    command_fields.add(&combo);
    command_fields.add(&custom_row);
    let command_error = error_label();
    let feedback = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .selectable(true)
        .max_width_chars(48)
        .accessible_role(gtk4::AccessibleRole::Status)
        .build();
    let spinner = adw::Spinner::new();
    spinner.set_size_request(20, 20);
    spinner.set_valign(gtk4::Align::Start);
    spinner.set_visible(false);
    let progress = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    progress.append(&spinner);
    progress.append(&feedback);
    feedback.set_hexpand(true);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    body.set_margin_top(12);
    body.set_margin_bottom(20);
    body.set_margin_start(20);
    body.set_margin_end(20);
    body.append(&workspace_fields);
    body.append(&command_fields);
    body.append(&command_error);
    body.append(&progress);
    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&body)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));
    let clamp = adw::Clamp::builder()
        .maximum_size(440)
        .tightening_threshold(400)
        .child(&toolbar)
        .build();
    dialog.set_child(Some(&clamp));
    dialog.set_default_widget(Some(&create));
    dialog.set_focus(Some(&browse));

    let form = Rc::new(NewWorkspaceDialog {
        dialog,
        pending: Cell::new(false),
        existing_workspace_id: RefCell::new(None),
        workspace_fields,
        command_fields,
        cancel,
        create,
        spinner,
        feedback,
        folder_row,
        folder_error,
        name_row,
        name_error,
        auto_name,
        combo,
        custom_row,
        command_error,
        state: RefCell::new(State {
            cwd,
            name_edited: false,
            setting_name: false,
        }),
        commands,
    });
    let weak = Rc::downgrade(&form);
    let submit = Rc::new(move || {
        if let Some(form) = weak.upgrade() {
            if let Some(request) = form.request() {
                on_create(request, form);
            }
        }
    });
    let confirm = submit.clone();
    form.create.connect_clicked(move |_| confirm());
    for row in [&form.name_row, &form.custom_row] {
        let confirm = submit.clone();
        row.connect_entry_activated(move |_| confirm());
    }
    let weak = Rc::downgrade(&form);
    form.cancel.connect_clicked(move |_| {
        if let Some(form) = weak.upgrade() {
            form.dialog.close();
        }
    });
    let weak = Rc::downgrade(&form);
    form.auto_name.connect_active_notify(move |row| {
        if let Some(form) = weak.upgrade() {
            form.name_row.set_visible(!row.is_active());
            if !row.is_active() {
                form.name_row.grab_focus();
            }
            form.validate();
        }
    });
    let weak = Rc::downgrade(&form);
    form.name_row.connect_changed(move |_| {
        if let Some(form) = weak.upgrade() {
            if !form.state.borrow().setting_name {
                form.state.borrow_mut().name_edited = true;
            }
            form.validate();
        }
    });
    let weak = Rc::downgrade(&form);
    form.custom_row.connect_changed(move |_| {
        if let Some(form) = weak.upgrade() {
            form.validate();
        }
    });
    let weak = Rc::downgrade(&form);
    form.combo.connect_selected_notify(move |combo| {
        if let Some(form) = weak.upgrade() {
            let sel = combo.selected() as usize;
            let supported = form
                .commands
                .get(sel)
                .is_some_and(|command| supports_auto_name(&command.argv));
            form.auto_name.set_sensitive(supported);
            form.auto_name.set_visible(supported);
            form.auto_name.set_subtitle(if supported {
                "From your first message"
            } else {
                "Available with Codex or Claude"
            });
            if !supported {
                form.auto_name.set_active(false);
            }
            form.custom_row.set_visible(sel >= form.commands.len());
            combo.set_subtitle(&form.commands.get(sel).map_or_else(
                || "Type any command below".to_string(),
                |command| command.argv.join(" "),
            ));
            form.validate();
        }
    });
    let weak = Rc::downgrade(&form);
    let picker_parent = parent.clone();
    let browse_action = Rc::new(move || {
        let Some(form) = weak.upgrade() else { return };
        let picker = gtk4::FileDialog::builder()
            .title("Choose a Project Folder")
            .initial_folder(&gio::File::for_path(&form.state.borrow().cwd))
            .build();
        let weak = Rc::downgrade(&form);
        picker.select_folder(
            Some(&picker_parent),
            None::<&gio::Cancellable>,
            move |result| {
                let Some(form) = weak.upgrade() else { return };
                let Ok(folder) = result else { return };
                let Some(path) = folder.path().map(|path| path.to_string_lossy().to_string())
                else {
                    return;
                };
                form.state.borrow_mut().cwd = path.clone();
                form.folder_row.set_subtitle(&tilde(&path));
                if !form.state.borrow().name_edited {
                    form.state.borrow_mut().setting_name = true;
                    form.name_row.set_text(&folder_basename(&path));
                    form.state.borrow_mut().setting_name = false;
                }
                form.validate();
            },
        );
    });
    let pick = browse_action.clone();
    form.folder_row.connect_activated(move |_| pick());
    browse.connect_clicked(move |_| browse_action());
    form.dialog.connect_closed(move |_| on_close());
    form.validate();
    form.dialog.present(Some(parent));
    form
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
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn automatic_workspace_launches_preserve_custom_commands() {
        use std::os::unix::fs::PermissionsExt;

        adw::init().unwrap();
        let directory = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
        std::fs::create_dir_all(&directory).unwrap();
        for program in ["codex", "claude", "agy"] {
            let file = directory.join(program);
            std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &directory);
        let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        for automatic in [false, true] {
            let form = show_dialog(&window, directory.to_str(), automatic, |_, _| {}, || {});
            for (program, expected) in [
                (
                    "codex",
                    vec!["codex", "--dangerously-bypass-approvals-and-sandbox"],
                ),
                ("claude", vec!["claude", "--dangerously-skip-permissions"]),
                (
                    "agy",
                    vec![
                        "agy",
                        "--dangerously-skip-permissions",
                        "--mode",
                        "accept-edits",
                    ],
                ),
            ] {
                let index = form
                    .commands
                    .iter()
                    .position(|command| command.argv[0] == program)
                    .unwrap();
                form.combo.set_selected(index as u32);
                let request = form.request().unwrap();
                let expected = if automatic { expected } else { vec![program] };
                assert_eq!(request.argv, expected);
                form.finish(Err(CreationError {
                    message: "retry fixture".into(),
                    workspace_id: None,
                }));
            }
            form.combo.set_selected(form.commands.len() as u32);
            form.custom_row.set_text("codex --sandbox read-only");
            assert_eq!(
                form.request().unwrap().argv,
                vec!["codex", "--sandbox", "read-only"]
            );
            form.finish(Ok(()));
        }
        std::fs::remove_file(directory.join("agy")).unwrap();
        for alias in ["antigravity", "antigravity-cli"] {
            let file = directory.join(alias);
            std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            let form = show_dialog(&window, directory.to_str(), true, |_, _| {}, || {});
            let index = form
                .commands
                .iter()
                .position(|command| command.argv[0] == alias)
                .unwrap();
            form.combo.set_selected(index as u32);
            assert_eq!(
                form.request().unwrap().argv,
                vec![
                    alias,
                    "--dangerously-skip-permissions",
                    "--mode",
                    "accept-edits"
                ]
            );
            form.finish(Ok(()));
            std::fs::remove_file(file).unwrap();
        }
        window.destroy();
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn dialog_validation_explains_invalid_folder() {
        adw::init().unwrap();
        let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        let dialog = show_dialog(
            &window,
            Some("/signaltty-missing-project-folder"),
            false,
            |_, _| panic!("invalid form submitted"),
            || {},
        );
        fn has_label(widget: &gtk4::Widget, text: &str) -> bool {
            if widget
                .downcast_ref::<gtk4::Label>()
                .is_some_and(|label| label.text() == text)
            {
                return true;
            }
            let mut child = widget.first_child();
            while let Some(widget) = child {
                if has_label(&widget, text) {
                    return true;
                }
                child = widget.next_sibling();
            }
            false
        }
        assert!(has_label(
            dialog.dialog.upcast_ref(),
            "Choose an existing project folder."
        ));
        assert!(!dialog.create.is_sensitive());
        dialog.create.emit_clicked();
        dialog.name_row.emit_by_name::<()>("entry-activated", &[]);
        assert!(!dialog.pending.get());
        dialog.dialog.close();
        window.destroy();
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

        type Captured = Rc<RefCell<Option<(String, String, Vec<String>, bool)>>>;
        let got: Captured = Rc::new(RefCell::new(None));
        let got2 = Rc::clone(&got);
        let submissions = Rc::new(Cell::new(0));
        let submitted = submissions.clone();
        let closed = Rc::new(Cell::new(false));
        let closed2 = Rc::clone(&closed);
        let dialog = show_dialog(
            &window,
            Some(&dir_s),
            false,
            move |req, _form| {
                submitted.set(submitted.get() + 1);
                *got2.borrow_mut() = Some((req.name, req.cwd, req.argv, req.auto_name));
            },
            move || closed2.set(true),
        );
        dialog.create.emit_clicked();
        while glib::MainContext::default().iteration(false) {}

        let commands = available_commands();
        let expected_argv = commands[default_command_index(&commands)].argv.clone();
        assert_eq!(
            got.borrow().clone(),
            Some((
                "signaltty-dialog-e2e".to_string(),
                dir_s,
                expected_argv.clone(),
                supports_auto_name(&expected_argv)
            ))
        );
        assert!(
            !closed.get(),
            "the form must remain open until creation succeeds"
        );
        assert!(dialog.pending.get());
        assert!(!dialog.dialog.can_close());
        dialog.create.emit_clicked();
        dialog.name_row.emit_by_name::<()>("entry-activated", &[]);
        dialog.cancel.emit_clicked();
        assert_eq!(submissions.get(), 1);
        assert!(!closed.get());
        dialog.finish(Err(CreationError {
            message: "Try again".into(),
            workspace_id: None,
        }));
        assert!(dialog.dialog.can_close());
        assert_eq!(dialog.create.label().as_deref(), Some("Retry"));
        assert_eq!(dialog.name_row.text(), "signaltty-dialog-e2e");
        dialog.create.emit_clicked();
        assert_eq!(submissions.get(), 2);
        dialog.finish(Ok(()));
        while glib::MainContext::default().iteration(false) {}
        assert!(closed.get());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

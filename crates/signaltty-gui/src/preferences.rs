//! Preferences dialog and persistence for appearance and theme settings.

use std::path::{Path, PathBuf};

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use signaltty_core::theme::{Appearance, GuiPreference, Theme};

/// Path to the persisted GUI preferences JSON file (`$XDG_CONFIG_HOME/signaltty/gui.json`).
pub fn config_path() -> PathBuf {
    signaltty_core::paths::config_dir().join("gui.json")
}

/// Load preference from a specific path, falling back to default silently on missing or corrupt files.
pub fn load_preference_from(path: &Path) -> GuiPreference {
    match std::fs::read_to_string(path) {
        Ok(contents) => GuiPreference::parse(&contents),
        Err(_) => GuiPreference::default(),
    }
}

/// Save preference to a specific path, creating parent directories and logging errors without crashing.
pub fn save_preference_to(path: &Path, pref: &GuiPreference) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("failed to create config dir {}: {e}", parent.display());
            return;
        }
    }
    if let Err(e) = std::fs::write(path, pref.to_json()) {
        tracing::warn!("failed to write gui preferences to {}: {e}", path.display());
    }
}

/// Load persisted preference from standard location.
pub fn load_preference() -> GuiPreference {
    load_preference_from(&config_path())
}

/// Save preference to standard location.
pub fn save_preference(pref: &GuiPreference) {
    save_preference_to(&config_path(), pref);
}

/// Mark `chosen` selected and its siblings not. Walking the parent
/// instead of capturing the button list keeps callbacks free of cycles.
fn select_only(chosen: &impl IsA<gtk4::Widget>) {
    let Some(parent) = chosen.parent() else {
        return;
    };
    let mut child = parent.first_child();
    while let Some(w) = child {
        if w == *chosen.upcast_ref() {
            w.add_css_class("selected");
        } else {
            w.remove_css_class("selected");
        }
        child = w.next_sibling();
    }
}

/// A miniature window in the scheme it selects: sidebar, content with
/// text lines, a side card and the input's accent dot. System splits the
/// content down the middle, light on the left and dark on the right.
fn scheme_preview(appearance: Appearance) -> gtk4::Box {
    let (sidebar, left, right) = match appearance {
        Appearance::System => ("light", "light", "dark"),
        Appearance::Light => ("light", "light", "light"),
        Appearance::Dark => ("dark", "dark", "dark"),
    };
    // GTK does not inherit text direction, so every part is pinned
    // left to right: the CSS (gradient split, input ends) and the End
    // alignments of the side card and accent dot are drawn that way.
    let part = |orientation, classes: &[&str]| {
        let b = gtk4::Box::new(orientation, 4);
        b.set_direction(gtk4::TextDirection::Ltr);
        for class in classes {
            b.add_css_class(class);
        }
        b
    };
    let bar = |classes: &[&str]| {
        let b = part(gtk4::Orientation::Horizontal, classes);
        b.add_css_class("mw-bar");
        b
    };

    let side = part(gtk4::Orientation::Vertical, &["mw-sidebar", sidebar]);
    for width in ["wide", "mid", "mid"] {
        side.append(&bar(&[width]));
    }

    let start = part(
        gtk4::Orientation::Vertical,
        &["mw-content", "mw-start", left],
    );
    start.set_hexpand(true);
    start.append(&bar(&["wide", "mw-title"]));
    start.append(&bar(&["long"]));
    start.append(&bar(&["mid"]));
    let spacer = part(gtk4::Orientation::Vertical, &[]);
    spacer.set_vexpand(true);
    start.append(&spacer);
    start.append(&part(gtk4::Orientation::Horizontal, &["mw-input"]));

    let end = part(
        gtk4::Orientation::Vertical,
        &["mw-content", "mw-end", right],
    );
    end.set_hexpand(true);
    let panel = part(gtk4::Orientation::Vertical, &["mw-panel"]);
    panel.set_halign(gtk4::Align::End);
    for width in ["mid", "short", "short"] {
        panel.append(&bar(&[width]));
    }
    end.append(&panel);
    let spacer = part(gtk4::Orientation::Vertical, &[]);
    spacer.set_vexpand(true);
    end.append(&spacer);
    let input = part(gtk4::Orientation::Horizontal, &["mw-input", "mw-input-end"]);
    let dot = part(gtk4::Orientation::Horizontal, &["mw-dot"]);
    dot.set_halign(gtk4::Align::End);
    dot.set_hexpand(true);
    input.append(&dot);
    end.append(&input);

    let scheme = match appearance {
        Appearance::System => "system",
        Appearance::Light => "light",
        Appearance::Dark => "dark",
    };
    // The halves share what the sidebar leaves equally, whatever their
    // content asks for, so the System split lands where its
    // calc(21px + 50%) background does.
    let halves = part(gtk4::Orientation::Horizontal, &["mw-halves"]);
    halves.set_spacing(0);
    halves.set_homogeneous(true);
    halves.set_hexpand(true);
    halves.append(&start);
    halves.append(&end);

    let preview = part(gtk4::Orientation::Horizontal, &["scheme-preview", scheme]);
    preview.set_spacing(0);
    preview.set_overflow(gtk4::Overflow::Hidden);
    preview.set_size_request(148, 92);
    preview.append(&side);
    preview.append(&halves);
    preview
}

/// Paired miniature checkouts preview the sidebar selection and terminal surface.
fn theme_preview(theme: Theme, variant: &str) -> gtk4::Box {
    let preview = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    preview.add_css_class("theme-preview");
    preview.add_css_class(theme.css_class());
    preview.add_css_class(variant);
    preview.set_direction(gtk4::TextDirection::Ltr);
    preview.set_overflow(gtk4::Overflow::Hidden);
    let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    sidebar.add_css_class("theme-preview-sidebar");
    for selected in [false, true, false] {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        line.add_css_class("theme-preview-line");
        if selected {
            line.add_css_class("theme-preview-selection");
        }
        sidebar.append(&line);
    }
    let terminal = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    terminal.add_css_class("theme-preview-terminal");
    terminal.set_hexpand(true);
    for width in [24, 16, 20] {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        line.add_css_class("theme-preview-line");
        line.set_size_request(width, -1);
        line.set_halign(gtk4::Align::Start);
        terminal.append(&line);
    }
    preview.append(&sidebar);
    preview.append(&terminal);
    preview
}

/// Build the Preferences dialog with appearance and agent launch controls.
pub fn build_dialog(app: &crate::app::App) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    dialog.add_css_class("signaltty-dialog");
    dialog.set_title("Preferences");

    let page = adw::PreferencesPage::new();
    page.set_title("Preferences");

    // --- Appearance Group ---
    let appearance_group = adw::PreferencesGroup::new();
    appearance_group.set_title("Appearance");

    let appearance_box = adw::WrapBox::new();
    appearance_box.set_wrap_policy(adw::WrapPolicy::Minimum);
    appearance_box.set_natural_line_length(600);
    appearance_box.set_child_spacing(6);
    appearance_box.set_line_spacing(12);
    appearance_box.set_justify(adw::JustifyMode::Fill);
    appearance_box.set_justify_last_line(false);
    appearance_box.set_halign(gtk4::Align::Fill);

    let current_pref = app.preference();

    let mut appearance_buttons = Vec::new();
    for app_variant in [Appearance::System, Appearance::Light, Appearance::Dark] {
        let btn = gtk4::ToggleButton::new();
        let card_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        card_box.append(&scheme_preview(app_variant));
        card_box.append(&gtk4::Label::new(Some(app_variant.label())));
        btn.set_child(Some(&card_box));
        btn.add_css_class("card");
        btn.add_css_class("appearance-card");
        btn.update_property(&[gtk4::accessible::Property::Label(app_variant.label())]);
        if current_pref.appearance == app_variant {
            btn.set_active(true);
            btn.add_css_class("selected");
        }
        if let Some(first) = appearance_buttons.first() {
            btn.set_group(Some(first));
        }
        appearance_box.append(&btn);
        appearance_buttons.push(btn);
    }

    let weak_app = app.weak();
    for (i, app_variant) in [Appearance::System, Appearance::Light, Appearance::Dark]
        .into_iter()
        .enumerate()
    {
        let btn = &appearance_buttons[i];
        let weak = weak_app.clone();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                select_only(b);
                if let Some(a) = weak.upgrade() {
                    a.set_appearance(app_variant);
                }
            }
        });
    }

    appearance_group.add(&appearance_box);

    // --- Theme Group ---
    let theme_group = adw::PreferencesGroup::new();
    theme_group.set_title("Theme");

    let theme_box = adw::WrapBox::new();
    theme_box.set_wrap_policy(adw::WrapPolicy::Minimum);
    theme_box.set_natural_line_length(600);
    theme_box.set_child_spacing(8);
    theme_box.set_line_spacing(12);
    theme_box.set_justify(adw::JustifyMode::Fill);
    theme_box.set_justify_last_line(false);
    theme_box.set_halign(gtk4::Align::Fill);

    let mut theme_buttons = Vec::new();
    for theme in Theme::ALL {
        let btn = gtk4::Button::new();
        btn.add_css_class("card");
        btn.add_css_class("theme-card");
        btn.update_property(&[gtk4::accessible::Property::Label(theme.label())]);

        let card_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        card_box.set_halign(gtk4::Align::Center);
        card_box.set_valign(gtk4::Align::Center);

        let swatches_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        swatches_box.set_halign(gtk4::Align::Center);
        for variant in ["light", "dark"] {
            swatches_box.append(&theme_preview(theme, variant));
        }

        let label = gtk4::Label::new(Some(theme.label()));

        card_box.append(&swatches_box);
        card_box.append(&label);
        btn.set_child(Some(&card_box));

        if current_pref.theme == theme {
            btn.add_css_class("selected");
        }

        theme_box.append(&btn);
        theme_buttons.push(btn);
    }

    let weak_app = app.weak();
    for (i, theme) in Theme::ALL.into_iter().enumerate() {
        let btn = &theme_buttons[i];
        let weak = weak_app.clone();
        btn.connect_clicked(move |b| {
            select_only(b);
            if let Some(a) = weak.upgrade() {
                a.set_theme(theme);
            }
        });
    }

    theme_group.add(&theme_box);

    page.add(&appearance_group);
    page.add(&theme_group);
    let agents = adw::PreferencesGroup::builder().title("Agents").build();
    let automatic = adw::SwitchRow::builder()
        .title("Run agents without confirmations")
        .subtitle("New Codex, Claude and Antigravity workspaces can run commands and edit files without asking. Codex runs without a sandbox.")
        .active(current_pref.automatic_agent_permissions)
        .build();
    let weak = app.weak();
    automatic.connect_active_notify(move |row| {
        if let Some(app) = weak.upgrade() {
            app.set_automatic_agent_permissions(row.is_active());
        }
    });
    agents.add(&automatic);
    page.add(&agents);
    dialog.add(&page);

    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unique_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "signaltty-test-pref-{}-{}",
            std::process::id(),
            signaltty_core::ids::new_pane_id()
        ))
    }

    /// The terminal background (`Theme::pane_bg`) must be the colour the
    /// stylesheet paints the pane card with, or the grid shows as a slab.
    #[test]
    fn pane_bg_table_matches_the_stylesheet() {
        let css = include_str!("../data/style.css");
        let mut found = std::collections::HashMap::new();
        for block in css.split('}') {
            let Some((head, body)) = block.split_once('{') else {
                continue;
            };
            let selector = head.lines().last().unwrap_or_default().trim();
            if let Some(value) = body
                .lines()
                .find_map(|l| l.trim().strip_prefix("--pane-bg-color:"))
            {
                found.insert(
                    selector.to_string(),
                    value.trim().trim_end_matches(';').to_string(),
                );
            }
        }
        for theme in Theme::ALL {
            for dark in [false, true] {
                let selector = match (theme, dark) {
                    (Theme::Signal, false) => ":root".to_string(),
                    (Theme::Signal, true) => ".dark".to_string(),
                    (t, false) => format!(".{}", t.css_class()),
                    (t, true) => format!(".{}.dark", t.css_class()),
                };
                assert_eq!(
                    found.get(&selector).map(String::as_str),
                    Some(theme.pane_bg(dark)),
                    "{selector}"
                );
            }
        }
    }

    #[test]
    fn load_preference_missing_file_returns_default() {
        let dir = test_unique_dir();
        let file = dir.join("gui.json");
        let pref = load_preference_from(&file);
        assert_eq!(pref, GuiPreference::default());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = test_unique_dir();
        let file = dir.join("gui.json");
        let pref = GuiPreference {
            appearance: Appearance::Dark,
            theme: Theme::Ocean,
            ..Default::default()
        };
        save_preference_to(&file, &pref);
        let loaded = load_preference_from(&file);
        assert_eq!(loaded, pref);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_roundtrip_with_sidebar_width() {
        let dir = test_unique_dir();
        let file = dir.join("gui.json");
        let pref = GuiPreference {
            appearance: Appearance::Dark,
            theme: Theme::Grove,
            sidebar_width: Some(380),
            ..Default::default()
        };
        save_preference_to(&file, &pref);
        let loaded = load_preference_from(&file);
        assert_eq!(loaded, pref);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_preference_corrupt_file_returns_default() {
        let dir = test_unique_dir();
        let file = dir.join("gui.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "corrupt non-json").unwrap();
        let loaded = load_preference_from(&file);
        assert_eq!(loaded, GuiPreference::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_preference_per_axis_fallback() {
        let dir = test_unique_dir();
        let file = dir.join("gui.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, r#"{"appearance": "invalid", "theme": "ember"}"#).unwrap();
        let loaded = load_preference_from(&file);
        assert_eq!(
            loaded,
            GuiPreference {
                appearance: Appearance::System,
                theme: Theme::Ember,
                ..Default::default()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_preference_unwritable_dir_logs_warning_and_does_not_crash() {
        let path = Path::new("/proc/nonexistent/gui.json");
        save_preference_to(path, &GuiPreference::default());
    }
}

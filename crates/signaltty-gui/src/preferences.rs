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

/// Build the Preferences dialog with Appearance and Theme controls.
pub fn build_dialog(app: &crate::app::App) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Preferences");

    let page = adw::PreferencesPage::new();
    page.set_title("Preferences");

    // --- Appearance Group ---
    let appearance_group = adw::PreferencesGroup::new();
    appearance_group.set_title("Appearance");

    let appearance_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    appearance_box.set_homogeneous(true);
    appearance_box.set_halign(gtk4::Align::Fill);

    let current_pref = app.preference();

    let mut appearance_buttons = Vec::new();
    for app_variant in [Appearance::System, Appearance::Light, Appearance::Dark] {
        let btn = gtk4::ToggleButton::new();
        // Neutral previews: System shows both halves, the others one.
        let preview = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        preview.set_halign(gtk4::Align::Center);
        let variants: &[&str] = match app_variant {
            Appearance::System => &["light", "dark"],
            Appearance::Light => &["light"],
            Appearance::Dark => &["dark"],
        };
        for variant in variants {
            let swatch = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            swatch.add_css_class("theme-swatch");
            swatch.add_css_class(Theme::Signal.css_class());
            swatch.add_css_class(variant);
            swatch.set_size_request(24, 24);
            preview.append(&swatch);
        }
        let card_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        card_box.set_halign(gtk4::Align::Center);
        card_box.append(&preview);
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
        let buttons = appearance_buttons.clone();
        let weak = weak_app.clone();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                for other in &buttons {
                    if other == b {
                        other.add_css_class("selected");
                    } else {
                        other.remove_css_class("selected");
                    }
                }
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

    let theme_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    theme_box.set_homogeneous(true);
    theme_box.set_halign(gtk4::Align::Fill);

    let mut theme_buttons = Vec::new();
    for theme in Theme::ALL {
        let btn = gtk4::Button::new();
        btn.add_css_class("card");
        btn.add_css_class("theme-card");
        btn.update_property(&[gtk4::accessible::Property::Label(theme.label())]);

        let card_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        card_box.set_halign(gtk4::Align::Center);
        card_box.set_valign(gtk4::Align::Center);

        // Dual swatches container
        let swatches_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        swatches_box.set_halign(gtk4::Align::Center);

        let light_swatch = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        light_swatch.add_css_class("theme-swatch");
        light_swatch.add_css_class(theme.css_class());
        light_swatch.add_css_class("light");
        light_swatch.set_size_request(24, 24);

        let dark_swatch = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dark_swatch.add_css_class("theme-swatch");
        dark_swatch.add_css_class(theme.css_class());
        dark_swatch.add_css_class("dark");
        dark_swatch.set_size_request(24, 24);

        swatches_box.append(&light_swatch);
        swatches_box.append(&dark_swatch);

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
        let buttons = theme_buttons.clone();
        let weak = weak_app.clone();
        btn.connect_clicked(move |b| {
            for other in &buttons {
                if other == b {
                    other.add_css_class("selected");
                } else {
                    other.remove_css_class("selected");
                }
            }
            if let Some(a) = weak.upgrade() {
                a.set_theme(theme);
            }
        });
    }

    theme_group.add(&theme_box);

    page.add(&appearance_group);
    page.add(&theme_group);
    dialog.add(&page);

    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unique_dir() -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("signaltty-test-pref-{}-{}", std::process::id(), ts))
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

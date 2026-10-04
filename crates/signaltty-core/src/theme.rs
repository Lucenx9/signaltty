//! GUI appearance and theme domain types, palettes, and preference parsing.
//!
//! Pure domain logic: no toolkit, no async, no I/O.

use serde::{Deserialize, Serialize};

/// Bounds for the user-dragged sidebar width, in px.
pub const SIDEBAR_MIN_WIDTH: u32 = 200;
pub const SIDEBAR_MAX_WIDTH: u32 = 560;

pub fn clamp_sidebar_width(w: f64) -> u32 {
    if w.is_nan() || w < SIDEBAR_MIN_WIDTH as f64 {
        SIDEBAR_MIN_WIDTH
    } else if w > SIDEBAR_MAX_WIDTH as f64 {
        SIDEBAR_MAX_WIDTH
    } else {
        w.round() as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn id(&self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "system" => Some(Appearance::System),
            "light" => Some(Appearance::Light),
            "dark" => Some(Appearance::Dark),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Signal,
    Grove,
    Ocean,
    Ember,
    Iris,
}

impl Theme {
    pub const ALL: [Theme; 5] = [
        Theme::Signal,
        Theme::Grove,
        Theme::Ocean,
        Theme::Ember,
        Theme::Iris,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Theme::Signal => "signal",
            Theme::Grove => "grove",
            Theme::Ocean => "ocean",
            Theme::Ember => "ember",
            Theme::Iris => "iris",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "signal" => Some(Theme::Signal),
            "grove" => Some(Theme::Grove),
            "ocean" => Some(Theme::Ocean),
            "ember" => Some(Theme::Ember),
            "iris" => Some(Theme::Iris),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Theme::Signal => "Signal",
            Theme::Grove => "Grove",
            Theme::Ocean => "Ocean",
            Theme::Ember => "Ember",
            Theme::Iris => "Iris",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Theme::Signal => "theme-signal",
            Theme::Grove => "theme-grove",
            Theme::Ocean => "theme-ocean",
            Theme::Ember => "theme-ember",
            Theme::Iris => "theme-iris",
        }
    }

    pub fn pane_bg(&self, is_dark: bool) -> &'static str {
        match (self, is_dark) {
            (Theme::Signal, false) => "#ffffff",
            (Theme::Signal, true) => "#111114",
            (Theme::Grove, false) => "#effdfd",
            (Theme::Grove, true) => "#071414",
            (Theme::Ocean, false) => "#f3fbff",
            (Theme::Ocean, true) => "#091318",
            (Theme::Ember, false) => "#fff8f5",
            (Theme::Ember, true) => "#180f0b",
            (Theme::Iris, false) => "#fbf8ff",
            (Theme::Iris, true) => "#131018",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GuiPreference {
    pub appearance: Appearance,
    pub theme: Theme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u32>,
}

impl GuiPreference {
    pub fn parse(json: &str) -> Self {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return Self::default();
        };
        let Some(obj) = value.as_object() else {
            return Self::default();
        };
        let appearance = obj
            .get("appearance")
            .and_then(|v| v.as_str())
            .and_then(Appearance::from_id)
            .unwrap_or_default();
        let theme = obj
            .get("theme")
            .and_then(|v| v.as_str())
            .and_then(Theme::from_id)
            .unwrap_or_default();
        let sidebar_width = obj
            .get("sidebar_width")
            .and_then(|v| v.as_f64())
            .map(clamp_sidebar_width);
        Self {
            appearance,
            theme,
            sidebar_width,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            format!(
                "{{\n  \"appearance\": \"{}\",\n  \"theme\": \"{}\"\n}}",
                self.appearance.id(),
                self.theme.id()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_id_roundtrip() {
        for app in [Appearance::System, Appearance::Light, Appearance::Dark] {
            let id = app.id();
            assert_eq!(Appearance::from_id(id), Some(app));
        }
        assert_eq!(Appearance::from_id("unknown"), None);
    }

    #[test]
    fn appearance_labels() {
        assert_eq!(Appearance::System.label(), "System");
        assert_eq!(Appearance::Light.label(), "Light");
        assert_eq!(Appearance::Dark.label(), "Dark");
    }

    #[test]
    fn theme_all_contains_five_themes() {
        assert_eq!(Theme::ALL.len(), 5);
        assert_eq!(
            Theme::ALL,
            [
                Theme::Signal,
                Theme::Grove,
                Theme::Ocean,
                Theme::Ember,
                Theme::Iris
            ]
        );
    }

    #[test]
    fn theme_id_roundtrip_and_labels() {
        for theme in Theme::ALL {
            let id = theme.id();
            assert_eq!(Theme::from_id(id), Some(theme));
        }
        assert_eq!(Theme::from_id("unknown"), None);
        assert_eq!(Theme::Signal.label(), "Signal");
        assert_eq!(Theme::Grove.label(), "Grove");
        assert_eq!(Theme::Ocean.label(), "Ocean");
        assert_eq!(Theme::Ember.label(), "Ember");
        assert_eq!(Theme::Iris.label(), "Iris");
    }

    #[test]
    fn theme_css_classes() {
        assert_eq!(Theme::Signal.css_class(), "theme-signal");
        assert_eq!(Theme::Grove.css_class(), "theme-grove");
        assert_eq!(Theme::Ocean.css_class(), "theme-ocean");
        assert_eq!(Theme::Ember.css_class(), "theme-ember");
        assert_eq!(Theme::Iris.css_class(), "theme-iris");
    }

    #[test]
    fn pane_bg_exact_hex_matrix() {
        // From research.md §1 & prompt:
        // signal #ffffff / #111114; grove #effdfd / #071414; ocean #f3fbff / #091318;
        // ember #fff8f5 / #180f0b; iris #fbf8ff / #131018.
        assert_eq!(Theme::Signal.pane_bg(false), "#ffffff");
        assert_eq!(Theme::Signal.pane_bg(true), "#111114");

        assert_eq!(Theme::Grove.pane_bg(false), "#effdfd");
        assert_eq!(Theme::Grove.pane_bg(true), "#071414");

        assert_eq!(Theme::Ocean.pane_bg(false), "#f3fbff");
        assert_eq!(Theme::Ocean.pane_bg(true), "#091318");

        assert_eq!(Theme::Ember.pane_bg(false), "#fff8f5");
        assert_eq!(Theme::Ember.pane_bg(true), "#180f0b");

        assert_eq!(Theme::Iris.pane_bg(false), "#fbf8ff");
        assert_eq!(Theme::Iris.pane_bg(true), "#131018");
    }

    #[test]
    fn gui_preference_default() {
        let def = GuiPreference::default();
        assert_eq!(def.appearance, Appearance::System);
        assert_eq!(def.theme, Theme::Signal);
        assert_eq!(def.sidebar_width, None);
    }

    #[test]
    fn clamp_sidebar_width_bounds() {
        // Below range
        assert_eq!(clamp_sidebar_width(100.0), 200);
        assert_eq!(clamp_sidebar_width(0.0), 200);
        assert_eq!(clamp_sidebar_width(-50.0), 200);
        assert_eq!(clamp_sidebar_width(199.4), 200);
        assert_eq!(clamp_sidebar_width(f64::NAN), 200);

        // Above range
        assert_eq!(clamp_sidebar_width(560.6), 560);
        assert_eq!(clamp_sidebar_width(700.0), 560);
        assert_eq!(clamp_sidebar_width(10000.0), 560);

        // Inside range
        assert_eq!(clamp_sidebar_width(200.0), 200);
        assert_eq!(clamp_sidebar_width(320.0), 320);
        assert_eq!(clamp_sidebar_width(320.4), 320);
        assert_eq!(clamp_sidebar_width(320.6), 321);
        assert_eq!(clamp_sidebar_width(560.0), 560);
    }

    #[test]
    fn gui_preference_roundtrip() {
        let pref_without_width = GuiPreference {
            appearance: Appearance::Dark,
            theme: Theme::Ocean,
            ..Default::default()
        };
        let json = pref_without_width.to_json();
        let parsed = GuiPreference::parse(&json);
        assert_eq!(parsed, pref_without_width);
        assert_eq!(parsed.sidebar_width, None);

        let pref_with_width = GuiPreference {
            appearance: Appearance::Light,
            theme: Theme::Iris,
            sidebar_width: Some(340),
        };
        let json = pref_with_width.to_json();
        let parsed = GuiPreference::parse(&json);
        assert_eq!(parsed, pref_with_width);
        assert_eq!(parsed.sidebar_width, Some(340));
    }

    #[test]
    fn gui_preference_parse_sidebar_width() {
        // Valid within range
        let p1 = GuiPreference::parse(r#"{"sidebar_width": 320}"#);
        assert_eq!(p1.sidebar_width, Some(320));

        // Clamped below min
        let p2 = GuiPreference::parse(r#"{"sidebar_width": 100}"#);
        assert_eq!(p2.sidebar_width, Some(200));

        // Clamped above max
        let p3 = GuiPreference::parse(r#"{"sidebar_width": 1000}"#);
        assert_eq!(p3.sidebar_width, Some(560));

        // Missing field
        let p4 = GuiPreference::parse(r#"{"appearance": "dark"}"#);
        assert_eq!(p4.sidebar_width, None);

        // Non-number / garbage fields fall back to None
        assert_eq!(
            GuiPreference::parse(r#"{"sidebar_width": "garbage"}"#).sidebar_width,
            None
        );
        assert_eq!(
            GuiPreference::parse(r#"{"sidebar_width": true}"#).sidebar_width,
            None
        );
        assert_eq!(
            GuiPreference::parse(r#"{"sidebar_width": [300]}"#).sidebar_width,
            None
        );
        assert_eq!(
            GuiPreference::parse(r#"{"sidebar_width": {"width": 300}}"#).sidebar_width,
            None
        );
        assert_eq!(
            GuiPreference::parse(r#"{"sidebar_width": null}"#).sidebar_width,
            None
        );
    }

    #[test]
    fn gui_preference_parse_corrupt_json() {
        assert_eq!(
            GuiPreference::parse("not json at all"),
            GuiPreference::default()
        );
        assert_eq!(GuiPreference::parse("{"), GuiPreference::default());
        assert_eq!(GuiPreference::parse("42"), GuiPreference::default());
        assert_eq!(GuiPreference::parse("[]"), GuiPreference::default());
    }

    #[test]
    fn gui_preference_parse_per_axis_fallback() {
        // Unknown appearance keeps valid theme
        let p1 = GuiPreference::parse(r#"{"appearance": "neon", "theme": "grove"}"#);
        assert_eq!(
            p1,
            GuiPreference {
                appearance: Appearance::System,
                theme: Theme::Grove,
                ..Default::default()
            }
        );

        // Missing appearance keeps valid theme
        let p2 = GuiPreference::parse(r#"{"theme": "ember"}"#);
        assert_eq!(
            p2,
            GuiPreference {
                appearance: Appearance::System,
                theme: Theme::Ember,
                ..Default::default()
            }
        );

        // Unknown theme keeps valid appearance
        let p3 = GuiPreference::parse(r#"{"appearance": "light", "theme": "matrix"}"#);
        assert_eq!(
            p3,
            GuiPreference {
                appearance: Appearance::Light,
                theme: Theme::Signal,
                ..Default::default()
            }
        );

        // Missing theme keeps valid appearance
        let p4 = GuiPreference::parse(r#"{"appearance": "dark"}"#);
        assert_eq!(
            p4,
            GuiPreference {
                appearance: Appearance::Dark,
                theme: Theme::Signal,
                ..Default::default()
            }
        );

        // Extra unknown fields are ignored
        let p5 = GuiPreference::parse(
            r#"{"appearance": "dark", "theme": "iris", "font_size": 14, "cursor": "block"}"#,
        );
        assert_eq!(
            p5,
            GuiPreference {
                appearance: Appearance::Dark,
                theme: Theme::Iris,
                ..Default::default()
            }
        );
    }
}

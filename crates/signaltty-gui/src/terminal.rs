//! One pane: a card holding a status header and a VTE terminal fed
//! externally from the server stream (the PTY lives in the server,
//! never here).
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────────╮
//! │ ◌ claude  Working               Approval   ⫿ ⊟ ×   [Resume] │
//! │ terminal …                                                   │
//! ╰──────────────────────────────────────────────────────────────╯
//! ```
//!
//! Attention is a ring inside the card's edge (an inset shadow, so it
//! never resizes the terminal and a split can't clip it); focus marks
//! the card in multi-pane tabs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::json;
use vte4::prelude::*;

use signaltty_core::{Decision, Lifecycle, LiveState, Pane, Theme};

use crate::actor::IpcHandle;
use crate::sidebar::agent_name;
use crate::status::{self, AttentionBadge, LifecycleIndicator};

#[derive(Debug, Clone)]
pub enum PaneAction {
    SplitRight,
    SplitDown,
    Close,
    Resume,
    AnswerDecision {
        decision_id: String,
        option_id: String,
    },
}

/// What the inline decision bar shows for a pending decision.
/// Headless-tested seam: pure mapping, no widgets.
#[derive(Debug, PartialEq, Eq)]
pub struct DecisionRender {
    pub prompt: String,
    pub options: Vec<(String, String)>,
    /// No proven answer channel: prompt + hint, no buttons.
    pub read_only: bool,
}

/// Map a [`Decision`] to its bar contents. Prose-only attention never
/// reaches here (the server only sets structured decisions), so every
/// option listed is a real button.
pub fn decision_render(decision: &Decision) -> DecisionRender {
    DecisionRender {
        prompt: decision.prompt.clone(),
        options: if decision.answerable {
            decision
                .options
                .iter()
                .map(|o| (o.id.clone(), o.label.clone()))
                .collect()
        } else {
            Vec::new()
        },
        read_only: !decision.answerable,
    }
}

const PCRE2_MULTILINE: u32 = 0x00000400;

type ActionCallback = Box<dyn Fn(&str, PaneAction)>;

pub struct PaneCallbacks {
    pub on_focus: Box<dyn Fn(&str)>,
    pub on_action: ActionCallback,
}

struct TerminalSearch {
    root: gtk4::Box,
    entry: gtk4::SearchEntry,
    message: gtk4::Label,
    compiled: RefCell<String>,
}

pub struct PaneWidget {
    pub root: gtk4::Box,
    term: vte4::Terminal,
    search: TerminalSearch,
    lifecycle: LifecycleIndicator,
    title: gtk4::Label,
    subtitle: gtk4::Label,
    badge: AttentionBadge,
    resume: gtk4::Button,
    decision_bar: gtk4::Box,
    decision_prompt: gtk4::Label,
    decision_options: libadwaita::WrapBox,
    decision_hint: gtk4::Label,
    shown_decision: RefCell<Option<String>>,
    on_action: Rc<ActionCallback>,
    pane_id: String,
    actor: IpcHandle,
    live: Cell<bool>,
    last_size: Cell<(u16, u16)>,
}

impl PaneWidget {
    pub fn new(pane_id: &str, actor: IpcHandle, cb: PaneCallbacks) -> Rc<PaneWidget> {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("pane");
        root.set_overflow(gtk4::Overflow::Hidden);

        let lifecycle = LifecycleIndicator::new();
        let title = gtk4::Label::new(None);
        title.add_css_class("pane-title");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let subtitle = gtk4::Label::new(None);
        subtitle.add_css_class("pane-subtitle");
        subtitle.add_css_class("dimmed");
        subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        subtitle.set_xalign(0.0);
        subtitle.set_hexpand(true);
        let badge = AttentionBadge::new();

        let on_action = Rc::new(cb.on_action);
        let action_button = |icon: &str, tooltip: &str, action: PaneAction| {
            let b = gtk4::Button::from_icon_name(icon);
            b.add_css_class("flat");
            b.add_css_class("circular");
            b.set_tooltip_text(Some(tooltip));
            b.update_property(&[gtk4::accessible::Property::Label(
                tooltip.split(" (").next().unwrap_or(tooltip),
            )]);
            b.set_focus_on_click(false);
            let on_action = Rc::clone(&on_action);
            let pid = pane_id.to_string();
            // Clone across the inner closure: the outer builder stays `Fn`.
            let act = action.clone();
            b.connect_clicked(move |_| on_action(&pid, act.clone()));
            b
        };
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        actions.add_css_class("pane-actions");
        actions.append(&action_button(
            "signaltty-split-right-symbolic",
            "Split Right (Ctrl+Shift+E)",
            PaneAction::SplitRight,
        ));
        actions.append(&action_button(
            "signaltty-split-down-symbolic",
            "Split Down (Ctrl+Shift+O)",
            PaneAction::SplitDown,
        ));
        actions.append(&action_button(
            "window-close-symbolic",
            "Close Pane (Ctrl+Shift+W)",
            PaneAction::Close,
        ));
        let resume = gtk4::Button::with_label("Resume");
        resume.add_css_class("suggested-action");
        resume.add_css_class("resume-button");
        resume.set_valign(gtk4::Align::Center);
        resume.set_tooltip_text(Some("Restart this agent's session where it left off"));
        resume.set_visible(false);
        {
            let on_action = Rc::clone(&on_action);
            let pid = pane_id.to_string();
            resume.connect_clicked(move |_| on_action(&pid, PaneAction::Resume));
        }

        // Status (recedes on unfocused panes) | attention (never
        // recedes) | actions (on hover/focus).
        let info = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        info.add_css_class("pane-info");
        info.set_hexpand(true);
        info.append(&lifecycle.widget);
        info.append(&title);
        info.append(&subtitle);
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.add_css_class("pane-header");
        header.append(&info);
        header.append(&badge.widget);
        header.append(&resume);
        header.append(&actions);

        let term = vte4::Terminal::new();
        term.set_scrollback_lines(5000);
        term.set_allow_hyperlink(true);
        term.set_bold_is_bright(false);
        // The card paints the background (and the padding around the
        // grid), so terminals always match the surrounding theme.
        term.set_clear_background(false);
        term.set_vexpand(true);
        term.set_hexpand(true);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroller.set_child(Some(&term));
        scroller.set_vexpand(true);

        // Inline decision bar (directive 2): prompt + one button per
        // option, built once and toggled. The VTE widget is never touched
        // here, so the bar appearing never rebuilds the terminal.
        let decision_prompt = gtk4::Label::new(None);
        decision_prompt.add_css_class("decision-prompt");
        decision_prompt.set_xalign(0.0);
        decision_prompt.set_hexpand(true);
        decision_prompt.set_wrap(true);
        decision_prompt.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        decision_prompt.set_selectable(true);
        let decision_options = libadwaita::WrapBox::new();
        decision_options.set_child_spacing(6);
        decision_options.set_line_spacing(6);
        decision_options.add_css_class("decision-options");
        let decision_hint = gtk4::Label::new(Some("Answer in the terminal"));
        decision_hint.add_css_class("decision-hint");
        decision_hint.add_css_class("dimmed");
        decision_hint.set_xalign(0.0);
        let decision_bar = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        decision_bar.add_css_class("decision-bar");
        decision_bar.append(&decision_prompt);
        decision_bar.append(&decision_options);
        decision_bar.append(&decision_hint);
        decision_bar.set_visible(false);

        root.append(&header);
        root.append(&decision_bar);
        let search_root = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let search_controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        search_root.set_margin_start(8);
        search_root.set_margin_end(8);
        let search_entry = gtk4::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Find in terminal"));
        search_entry.set_hexpand(true);
        search_entry.set_width_chars(8);
        let search_message = gtk4::Label::new(None);
        search_message.add_css_class("dimmed");
        search_message.set_wrap(true);
        search_message.set_xalign(0.0);
        let previous = gtk4::Button::from_icon_name("go-up-symbolic");
        previous.set_tooltip_text(Some("Previous match (Shift+Enter)"));
        let next = gtk4::Button::from_icon_name("go-down-symbolic");
        next.set_tooltip_text(Some("Next match (Enter)"));
        let close_search = gtk4::Button::from_icon_name("window-close-symbolic");
        close_search.set_tooltip_text(Some("Close search (Escape)"));
        for button in [&previous, &next, &close_search] {
            button.add_css_class("flat");
            if let Some(label) = button.tooltip_text() {
                button.update_property(&[gtk4::accessible::Property::Label(&label)]);
            }
        }
        search_controls.append(&search_entry);
        search_controls.append(&previous);
        search_controls.append(&next);
        search_controls.append(&close_search);
        search_root.append(&search_controls);
        search_root.append(&search_message);
        search_root.set_visible(false);
        root.append(&search_root);
        root.append(&scroller);

        let w = Rc::new(PaneWidget {
            root,
            term,
            search: TerminalSearch {
                root: search_root,
                entry: search_entry,
                message: search_message,
                compiled: RefCell::new(String::new()),
            },
            lifecycle,
            title,
            subtitle,
            decision_bar,
            decision_prompt,
            decision_options,
            decision_hint,
            shown_decision: RefCell::new(None),
            on_action: Rc::clone(&on_action),
            badge,
            resume,
            pane_id: pane_id.to_string(),
            actor,
            live: Cell::new(true),
            last_size: Cell::new((80, 24)),
        });
        w.apply_style(Theme::Signal);
        w.term.search_set_wrap_around(true);
        let weak = Rc::downgrade(&w);
        w.search.entry.connect_changed(move |_| {
            if let Some(w) = weak.upgrade() {
                if w.search.root.is_visible() {
                    w.set_search();
                }
            }
        });
        let weak = Rc::downgrade(&w);
        next.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.find(false);
            }
        });
        let weak = Rc::downgrade(&w);
        previous.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.find(true);
            }
        });
        let weak = Rc::downgrade(&w);
        close_search.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.close_search();
            }
        });
        let weak = Rc::downgrade(&w);
        w.search.entry.connect_stop_search(move |_| {
            if let Some(w) = weak.upgrade() {
                w.close_search();
            }
        });
        let keys = gtk4::EventControllerKey::new();
        let weak = Rc::downgrade(&w);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(w) = weak.upgrade() else {
                return gtk4::glib::Propagation::Proceed;
            };
            if key == gtk4::gdk::Key::Return || key == gtk4::gdk::Key::KP_Enter {
                w.find(modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK));
                gtk4::glib::Propagation::Stop
            } else if key == gtk4::gdk::Key::Escape {
                w.close_search();
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        w.search.entry.add_controller(keys);

        // Input: VTE translates keys to bytes; forward to the server PTY.
        {
            let w = Rc::clone(&w);
            let term = w.term.clone();
            term.connect_commit(move |_, text: &str, _| {
                if !w.live.get() {
                    return;
                }
                use base64::Engine;
                w.send(
                    "pane.input",
                    json!({
                        "pane_id": w.pane_id,
                        "data_b64": base64::engine::general_purpose::STANDARD.encode(text.as_bytes()),
                    }),
                );
            });
        }
        // Focus = explicit per-pane interaction: clear attention.
        {
            let w = Rc::clone(&w);
            let on_focus = cb.on_focus;
            let term = w.term.clone();
            term.connect_has_focus_notify(move |term| {
                if term.has_focus() {
                    w.send("pane.mark_seen", json!({"pane_id": w.pane_id}));
                    on_focus(&w.pane_id);
                }
            });
        }
        // Clicking the header focuses the pane too.
        {
            let click = gtk4::GestureClick::new();
            let term = w.term.clone();
            click.connect_pressed(move |_, _, _, _| {
                term.grab_focus();
            });
            header.add_controller(click);
        }

        // Snapshots and live bytes arrive through the same ordered UI queue.
        w.actor.attach(pane_id, 80, 24);
        w
    }

    pub fn present_search(&self) {
        self.search.root.set_visible(true);
        self.search.entry.grab_focus();
    }

    pub fn close_search(&self) {
        self.search.root.set_visible(false);
        self.term.search_set_regex(None, 0);
        self.search.compiled.borrow_mut().clear();
        self.term.unselect_all();
        self.focus();
    }

    fn set_search(&self) -> bool {
        let text = self.search.entry.text();
        if !text.is_empty() && self.search.compiled.borrow().as_str() == text.as_str() {
            return true;
        }
        if text.is_empty() {
            self.term.search_set_regex(None, 0);
            self.search.message.set_text("");
            self.search.compiled.borrow_mut().clear();
            return false;
        }
        let mut pattern = String::new();
        for ch in text.chars() {
            if "\\.^$|?*+()[]{}".contains(ch) {
                pattern.push('\\');
            }
            pattern.push(ch);
        }
        match vte4::Regex::for_search(&pattern, PCRE2_MULTILINE) {
            Ok(regex) => {
                self.term.search_set_regex(Some(&regex), 0);
                *self.search.compiled.borrow_mut() = text.to_string();
                true
            }
            Err(_) => {
                self.term.search_set_regex(None, 0);
                self.search.message.set_text("Invalid search");
                false
            }
        }
    }

    pub fn find(&self, previous: bool) -> bool {
        if !self.set_search() {
            return false;
        }
        let found = if previous {
            self.term.search_find_previous()
        } else {
            self.term.search_find_next()
        };
        self.search
            .message
            .set_text(if found { "" } else { "No matches" });
        found
    }

    /// Font + palette from the desktop; re-run when either changes.
    pub fn apply_style(&self, theme: Theme) {
        let sm = libadwaita::StyleManager::default();
        let name = sm.monospace_font_name();
        let font = gtk4::pango::FontDescription::from_string(if name.is_empty() {
            "Monospace 11"
        } else {
            name.as_str()
        });
        self.term.set_font(Some(&font));
        let fg_hex = if sm.is_dark() {
            DARK_FOREGROUND
        } else {
            LIGHT_FOREGROUND
        };
        let rgba = |hex: &str| gtk4::gdk::RGBA::parse(hex).expect("palette colour");
        let palette: Vec<gtk4::gdk::RGBA> = PALETTE.iter().map(|c| rgba(c)).collect();
        let palette: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        self.term.set_colors(
            Some(&rgba(fg_hex)),
            Some(&rgba(theme.pane_bg(sm.is_dark()))),
            &palette,
        );
    }

    fn send(&self, method: &'static str, params: serde_json::Value) {
        let actor = self.actor.clone();
        gtk4::glib::spawn_future_local(async move {
            let _ = actor.call(method, params).await;
        });
    }

    pub fn replace_screen(&self, data: &[u8]) {
        self.term.reset(true, true);
        self.term.feed(data);
    }

    pub fn feed(&self, data: &[u8]) {
        self.term.feed(data);
    }

    pub fn focus(&self) {
        self.term.grab_focus();
    }

    /// Mark this pane as the target of pane actions.
    pub fn set_focused(&self, focused: bool) {
        if focused {
            self.root.add_css_class("focused");
        } else {
            self.root.remove_css_class("focused");
        }
    }

    pub fn detach(&self) {
        self.actor.detach(&self.pane_id);
    }

    /// Report widget-driven size when it changed (called on a 250ms tick;
    /// gtk4 0.11 has no size-allocate signal). Last-writer-wins.
    pub fn sync_size(&self) {
        if !self.root.is_mapped() {
            return;
        }
        let cols = self.term.column_count().clamp(20, 500) as u16;
        let rows = self.term.row_count().clamp(5, 200) as u16;
        if self.last_size.get() == (cols, rows) {
            return;
        }
        self.last_size.set((cols, rows));
        if self.live.get() {
            self.send(
                "pane.resize",
                json!({"pane_id": self.pane_id, "cols": cols, "rows": rows}),
            );
        }
    }

    /// Refresh header + ring from fresh pane state.
    pub fn update_meta(&self, pane: &Pane) {
        let live = matches!(pane.live, LiveState::Live);
        self.title.set_text(&pane.title);
        self.title.set_tooltip_text(Some(&pane.cwd));
        let mut context: Vec<String> = Vec::new();
        if let Some(agent) = agent_name(pane.agent.kind) {
            if !agent.eq_ignore_ascii_case(pane.title.trim()) {
                context.push(agent.to_string());
            }
        }
        match pane.live {
            LiveState::Exited { code: Some(code) } if code != 0 => {
                context.push(format!("Exited ({code})"))
            }
            LiveState::Exited { .. } => context.push("Exited".to_string()),
            LiveState::Live if pane.lifecycle != Lifecycle::Unknown => {
                context.push(status::lifecycle_label(pane.lifecycle).to_string())
            }
            LiveState::Live => {}
        }
        self.subtitle.set_text(&context.join(" · "));
        self.lifecycle.set(status::effective_lifecycle(pane));
        self.badge.set(pane.attention);
        status::set_attention_class(&self.root, pane.attention);

        let was_live = self.live.replace(live);
        if was_live && !live {
            self.term.feed(b"\r\n\x1b[2m[process exited]\x1b[0m\r\n");
        }
        // Resume affordance for restored/resumable tombstones.
        self.resume
            .set_visible(!live && pane.agent.resume_argv.is_some());
        self.sync_decision_bar(pane.pending_decision.as_ref());
    }

    /// Toggle the inline decision bar. Buttons rebuild only when the
    /// decision id changes; showing/hiding never touches the VTE.
    fn sync_decision_bar(&self, decision: Option<&Decision>) {
        match decision {
            None => {
                self.decision_bar.set_visible(false);
                *self.shown_decision.borrow_mut() = None;
            }
            Some(d) => {
                let rendered = decision_render(d);
                self.decision_prompt.set_text(&rendered.prompt);
                self.decision_prompt
                    .set_tooltip_text(Some(&rendered.prompt));
                self.decision_hint.set_visible(rendered.read_only);
                if self.shown_decision.borrow().as_deref() != Some(d.id.as_str()) {
                    while let Some(child) = self.decision_options.first_child() {
                        self.decision_options.remove(&child);
                    }
                    for (option_id, label) in &rendered.options {
                        let text = gtk4::Label::new(Some(label));
                        text.set_wrap(true);
                        text.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                        text.set_xalign(0.0);
                        let button = gtk4::Button::new();
                        button.set_child(Some(&text));
                        button.add_css_class("pill");
                        button.update_property(&[gtk4::accessible::Property::Label(label)]);
                        button.set_tooltip_text(Some(label));
                        button.set_focus_on_click(false);
                        let on_action = Rc::clone(&self.on_action);
                        let pane_id = self.pane_id.clone();
                        let decision_id = d.id.clone();
                        let option_id = option_id.clone();
                        button.connect_clicked(move |_| {
                            on_action(
                                &pane_id,
                                PaneAction::AnswerDecision {
                                    decision_id: decision_id.clone(),
                                    option_id: option_id.clone(),
                                },
                            );
                        });
                        self.decision_options.append(&button);
                    }
                    *self.shown_decision.borrow_mut() = Some(d.id.clone());
                }
                self.decision_bar.set_visible(true);
            }
        }
    }
}

const LIGHT_FOREGROUND: &str = "#241f31";
const DARK_FOREGROUND: &str = "#deddda";

/// GNOME palette (as in Console/Ptyxis): legible on both schemes.
const PALETTE: [&str; 16] = [
    "#241f31", "#c01c28", "#2ec27e", "#f5c211", "#1e78e4", "#9841bb", "#0ab9dc", "#c0bfbc",
    "#5e5c64", "#ed333b", "#57e389", "#f8e45c", "#51a1ff", "#c061cb", "#4fd2fd", "#f6f5f4",
];

#[cfg(test)]
mod tests {
    use super::*;
    use signaltty_core::DecisionOption;

    fn decision(answerable: bool) -> Decision {
        Decision {
            id: "d1".into(),
            prompt: "Allow rm -rf /tmp/x?".into(),
            options: vec![
                DecisionOption {
                    id: "once".into(),
                    label: "Once".into(),
                },
                DecisionOption {
                    id: "always".into(),
                    label: "Always".into(),
                },
                DecisionOption {
                    id: "deny".into(),
                    label: "Deny".into(),
                },
            ],
            answerable,
            received_at: chrono::Utc::now(),
        }
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn terminal_search_is_literal_and_keeps_the_terminal() {
        libadwaita::init().unwrap();
        let (actor, _requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "search",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let term = pane.term.clone();
        let window = gtk4::Window::new();
        window.set_child(Some(&pane.root));
        window.present();
        pane.feed(b"alpha [literal].* omega\r\nsecond [literal].*\r\n");
        for _ in 0..20 {
            while gtk4::glib::MainContext::default().iteration(false) {}
        }
        pane.present_search();
        pane.search.entry.set_text("[literal].*");
        assert!(pane.find(false));
        assert!(pane.term.has_selection());
        pane.search.entry.set_text("absent needle");
        assert!(!pane.find(false));
        assert_eq!(pane.search.message.text(), "No matches");
        pane.close_search();
        assert!(!pane.search.root.is_visible());
        pane.search.entry.set_text("hidden query");
        assert!(pane.term.search_get_regex().is_none());
        assert_eq!(pane.term, term);
        window.destroy();
    }

    #[test]
    fn render_lists_every_option_as_a_button() {
        let r = decision_render(&decision(true));
        assert_eq!(r.prompt, "Allow rm -rf /tmp/x?");
        assert_eq!(
            r.options,
            vec![
                ("once".to_string(), "Once".to_string()),
                ("always".to_string(), "Always".to_string()),
                ("deny".to_string(), "Deny".to_string()),
            ]
        );
        assert!(!r.read_only);
    }

    #[test]
    fn render_without_channel_is_read_only_with_no_buttons() {
        let r = decision_render(&decision(false));
        assert_eq!(r.prompt, "Allow rm -rf /tmp/x?");
        assert!(r.options.is_empty(), "never fake buttons");
        assert!(r.read_only);
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn approval_question_and_choices_fit_a_narrow_pane() {
        libadwaita::init().unwrap();
        gtk4::gio::resources_register_include!("signaltty-gui.gresource").unwrap();
        let display = gtk4::gdk::Display::default().unwrap();
        let provider = gtk4::CssProvider::new();
        provider.load_from_resource("/dev/signaltty/gui/style.css");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let (actor, _requests) = IpcHandle::test_channel();
        let selected = Rc::new(RefCell::new(None));
        let selection = Rc::clone(&selected);
        let widget = PaneWidget::new(
            "pane-narrow",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(move |pane, action| {
                    *selection.borrow_mut() = Some((pane.to_string(), action));
                }),
            },
        );
        let mut approval = decision(true);
        approval.prompt = "Run the workspace verification suite and inspect the native interface in light and dark themes?".into();
        approval.options[0].label = "Allow once".into();
        approval.options[1].label =
            "Allow this verification command for the current workspace session".into();
        let mut pane = Pane::new(
            "workspace".into(),
            "tab".into(),
            "/tmp".into(),
            vec!["codex".into()],
            Default::default(),
            chrono::Utc::now(),
        );
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        let minimum = widget.root.measure(gtk4::Orientation::Horizontal, -1).0;
        assert!(minimum <= 320, "approval forces a {minimum}px pane");

        let window = gtk4::Window::new();
        window.set_default_size(320, 520);
        window.set_child(Some(&widget.root));
        window.present();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while widget.root.width() == 0 {
            assert!(std::time::Instant::now() < deadline);
            gtk4::glib::MainContext::default().iteration(true);
        }
        assert!(widget.root.width() <= 320);
        assert!(!widget.decision_prompt.layout().is_ellipsized());
        assert!(widget.decision_prompt.layout().line_count() > 1);
        let mut child = widget.decision_options.first_child();
        let mut choices = Vec::new();
        while let Some(option) = child {
            let button = option.clone().downcast::<gtk4::Button>().unwrap();
            let bounds = button.compute_bounds(&widget.root).unwrap();
            assert!(bounds.x() >= 0.0 && bounds.x() + bounds.width() <= 320.0);
            let label = button.child().unwrap().downcast::<gtk4::Label>().unwrap();
            assert!(!label.layout().is_ellipsized());
            choices.push(button);
            child = option.next_sibling();
        }
        assert_eq!(choices.len(), 3);
        choices[1].emit_clicked();
        assert!(matches!(
            selected.borrow().as_ref(),
            Some((id, PaneAction::AnswerDecision { decision_id, option_id }))
                if id == "pane-narrow" && decision_id == "d1" && option_id == "always"
        ));
        let terminal = widget.term.clone();
        pane.pending_decision = None;
        widget.update_meta(&pane);
        assert_eq!(
            widget.term, terminal,
            "answering must retain the mounted VTE"
        );
        window.close();
        gtk4::style_context_remove_provider_for_display(&display, &provider);
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn keyboard_focus_renders_an_inactive_panes_controls() {
        libadwaita::init().unwrap();
        gtk4::gio::resources_register_include!("signaltty-gui.gresource").unwrap();
        let display = gtk4::gdk::Display::default().unwrap();
        let provider = gtk4::CssProvider::new();
        provider.load_from_resource("/dev/signaltty/gui/style.css");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let settings = gtk4::Settings::default().unwrap();
        settings.set_gtk_enable_animations(false);
        let (actor, _requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "inactive",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let window = gtk4::Window::new();
        window.set_default_size(420, 180);
        window.set_child(Some(&pane.root));
        window.present();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while pane.root.width() == 0 {
            assert!(std::time::Instant::now() < deadline);
            gtk4::glib::MainContext::default().iteration(true);
        }
        gtk4::prelude::GtkWindowExt::set_focus(&window, Some(&pane.term));
        pane.root.unset_state_flags(gtk4::StateFlags::PRELIGHT);
        let header = pane.root.first_child().unwrap();
        let actions = header.last_child().unwrap();
        let control = actions.first_child().unwrap();
        let bounds = actions.compute_bounds(&pane.root).unwrap();
        let pixels = || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(60);
            while std::time::Instant::now() < deadline {
                while gtk4::glib::MainContext::default().iteration(false) {}
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let paintable = gtk4::WidgetPaintable::new(Some(&pane.root));
            let snapshot = gtk4::Snapshot::new();
            paintable.snapshot(
                &snapshot,
                pane.root.width() as f64,
                pane.root.height() as f64,
            );
            let node = snapshot.to_node().unwrap();
            let renderer = gtk4::gsk::CairoRenderer::new();
            renderer.realize_for_display(&display).unwrap();
            let texture = renderer.render_texture(&node, Some(&bounds));
            renderer.unrealize();
            let stride = texture.width() as usize * 4;
            let mut bytes = vec![0; stride * texture.height() as usize];
            texture.download(&mut bytes, stride);
            bytes
        };
        let hidden = pixels();
        window.set_focus_visible(true);
        assert!(control.grab_focus());
        pane.root.unset_state_flags(gtk4::StateFlags::PRELIGHT);
        let focused = pixels();
        assert!(
            hidden != focused,
            "keyboard-focused controls remain invisible"
        );
        window.close();
        gtk4::style_context_remove_provider_for_display(&display, &provider);
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn reduced_motion_removes_rendered_press_scaling() {
        libadwaita::init().unwrap();
        gtk4::gio::resources_register_include!("signaltty-gui.gresource").unwrap();
        let display = gtk4::gdk::Display::default().unwrap();
        let provider = gtk4::CssProvider::new();
        provider.load_from_resource("/dev/signaltty/gui/style.css");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        gtk4::Settings::default()
            .unwrap()
            .set_gtk_enable_animations(false);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let button = gtk4::Button::with_label("Allow once");
        button.add_css_class("pill");
        button.set_focusable(false);
        root.append(&button);
        let window = gtk4::Window::new();
        window.set_child(Some(&root));
        window.present();
        button.set_state_flags(gtk4::StateFlags::ACTIVE, false);

        fn scaled(node: &gtk4::gsk::RenderNode) -> bool {
            use gtk4::gsk;
            if let Some(n) = node.downcast_ref::<gsk::TransformNode>() {
                let (xx, _, _, yy, _, _) = n.transform().to_2d();
                return (xx - 0.97).abs() < 0.001 && (yy - 0.97).abs() < 0.001
                    || scaled(&n.child());
            }
            if let Some(n) = node.downcast_ref::<gsk::ContainerNode>() {
                return (0..n.n_children()).any(|i| scaled(&n.child(i)));
            }
            if let Some(n) = node.downcast_ref::<gsk::ClipNode>() {
                return scaled(&n.child());
            }
            if let Some(n) = node.downcast_ref::<gsk::RoundedClipNode>() {
                return scaled(&n.child());
            }
            if let Some(n) = node.downcast_ref::<gsk::OpacityNode>() {
                return scaled(&n.child());
            }
            false
        }
        let snapshot = || {
            let end = std::time::Instant::now() + std::time::Duration::from_millis(80);
            while std::time::Instant::now() < end {
                while gtk4::glib::MainContext::default().pending() {
                    gtk4::glib::MainContext::default().iteration(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let paintable = gtk4::WidgetPaintable::new(Some(&root));
            let snapshot = gtk4::Snapshot::new();
            paintable.snapshot(&snapshot, root.width() as f64, root.height() as f64);
            snapshot.to_node().unwrap()
        };
        assert!(
            scaled(&snapshot()),
            "probe must observe normal press scaling"
        );
        root.add_css_class("reduced-motion");
        assert!(
            !scaled(&snapshot()),
            "reduced motion still scales the pressed button"
        );
        root.remove_css_class("reduced-motion");
        assert!(
            scaled(&snapshot()),
            "normal feedback must return immediately"
        );
        button.set_focusable(true);
        window.set_focus_visible(true);
        assert!(button.grab_focus());
        assert!(!scaled(&snapshot()), "keyboard activation must not scale");
        window.close();
        gtk4::style_context_remove_provider_for_display(&display, &provider);
    }
}

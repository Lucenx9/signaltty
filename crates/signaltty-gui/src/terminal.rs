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

use signaltty_core::{Decision, Lifecycle, LiveState, Pane};

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

type ActionCallback = Box<dyn Fn(&str, PaneAction)>;

pub struct PaneCallbacks {
    pub on_focus: Box<dyn Fn(&str)>,
    pub on_action: ActionCallback,
}

pub struct PaneWidget {
    pub root: gtk4::Box,
    term: vte4::Terminal,
    lifecycle: LifecycleIndicator,
    title: gtk4::Label,
    subtitle: gtk4::Label,
    badge: AttentionBadge,
    resume: gtk4::Button,
    decision_bar: gtk4::Box,
    decision_prompt: gtk4::Label,
    decision_options: gtk4::Box,
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
        decision_prompt.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let decision_options = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        decision_options.add_css_class("decision-options");
        let decision_hint = gtk4::Label::new(Some("Answer in the terminal"));
        decision_hint.add_css_class("decision-hint");
        decision_hint.add_css_class("dimmed");
        let decision_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        decision_bar.add_css_class("decision-bar");
        decision_bar.append(&decision_prompt);
        decision_bar.append(&decision_options);
        decision_bar.append(&decision_hint);
        decision_bar.set_visible(false);

        root.append(&header);
        root.append(&decision_bar);
        root.append(&scroller);

        let w = Rc::new(PaneWidget {
            root,
            term,
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
        w.apply_style();

        // Input: VTE translates keys to bytes; forward to the server PTY.
        {
            let w = Rc::clone(&w);
            let term = w.term.clone();
            term.connect_commit(move |_, text: &str, _| {
                if !w.live.get() {
                    return;
                }
                use base64::Engine;
                let _ = w.actor.call(
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
                    let _ = w
                        .actor
                        .call("pane.mark_seen", json!({"pane_id": w.pane_id}));
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

        // Attach + initial snapshot (replayable VT bytes).
        match w.actor.attach(pane_id, 80, 24) {
            Ok(snap) => {
                w.term.reset(true, true);
                if !snap.snapshot.is_empty() {
                    w.term.feed(&snap.snapshot);
                }
            }
            Err(e) => {
                w.term
                    .feed(format!("\r\n\x1b[2mCouldn't attach: {e}\x1b[0m\r\n").as_bytes());
            }
        }
        w
    }

    /// Font + palette from the desktop; re-run when either changes.
    pub fn apply_style(&self) {
        let sm = libadwaita::StyleManager::default();
        let name = sm.monospace_font_name();
        let font = gtk4::pango::FontDescription::from_string(if name.is_empty() {
            "Monospace 11"
        } else {
            name.as_str()
        });
        self.term.set_font(Some(&font));
        let scheme = if sm.is_dark() { &DARK } else { &LIGHT };
        let rgba = |hex: &str| gtk4::gdk::RGBA::parse(hex).expect("palette colour");
        let palette: Vec<gtk4::gdk::RGBA> = PALETTE.iter().map(|c| rgba(c)).collect();
        let palette: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        self.term.set_colors(
            Some(&rgba(scheme.foreground)),
            Some(&rgba(scheme.background)),
            &palette,
        );
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
        let cols = self.term.column_count().clamp(20, 500) as u16;
        let rows = self.term.row_count().clamp(5, 200) as u16;
        if self.last_size.get() == (cols, rows) {
            return;
        }
        self.last_size.set((cols, rows));
        if self.live.get() {
            let _ = self.actor.call(
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
                        let button = gtk4::Button::with_label(label);
                        button.add_css_class("pill");
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

struct Scheme {
    foreground: &'static str,
    /// Matches libadwaita's view background; used for reverse video
    /// (the card itself paints the default background).
    background: &'static str,
}

const LIGHT: Scheme = Scheme {
    foreground: "#241f31",
    background: "#ffffff",
};

const DARK: Scheme = Scheme {
    foreground: "#deddda",
    background: "#1d1d20",
};

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
}

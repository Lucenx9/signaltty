//! One pane: a card holding a status header and a VTE terminal fed
//! externally from the server stream (the PTY lives in the server,
//! never here).
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────────╮
//! │ ◌ worker Idle [Task Working]    Approval   ⫿ ⊟ ×   [Resume] │
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
use crate::task_chip::{TaskChipView, TaskChipWidget};

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

/// The one suggested answer on a gate: allow once. Session-scoped allow
/// and deny stay secondary so the bar has a single primary.
fn decision_is_primary(id: &str, label: &str) -> bool {
    let id = id.trim();
    let label = label.trim();
    id.eq_ignore_ascii_case("once")
        || label.eq_ignore_ascii_case("allow once")
        || label.eq_ignore_ascii_case("once")
}

const PCRE2_MULTILINE: u32 = 0x00000400;

/// The menu item's enabled flag is not the shortcut result: a disabled
/// `SimpleAction` reports the key as unhandled and VTE would deliver
/// Ctrl+C or Ctrl+V.
#[derive(Clone, Copy)]
enum TerminalCommand {
    Copy,
    Paste,
    IncreaseTextSize,
    DecreaseTextSize,
    ResetTextSize,
}

const TEXT_TENTH_MIN: i32 = -5;
const TEXT_TENTH_MAX: i32 = 10;

fn text_factor(tenths: i32) -> f64 {
    f64::from(10 + tenths) / 10.0
}

/// The keypad and synthetic key events may use shifted aliases.
fn terminal_chord(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> Option<TerminalCommand> {
    if !modifiers.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
        || !modifiers.contains(gtk4::gdk::ModifierType::SHIFT_MASK)
        || modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK)
        || modifiers.contains(gtk4::gdk::ModifierType::SUPER_MASK)
    {
        return None;
    }
    match key {
        gtk4::gdk::Key::c | gtk4::gdk::Key::C => Some(TerminalCommand::Copy),
        gtk4::gdk::Key::v | gtk4::gdk::Key::V => Some(TerminalCommand::Paste),
        gtk4::gdk::Key::plus | gtk4::gdk::Key::equal | gtk4::gdk::Key::KP_Add => {
            Some(TerminalCommand::IncreaseTextSize)
        }
        gtk4::gdk::Key::minus | gtk4::gdk::Key::underscore | gtk4::gdk::Key::KP_Subtract => {
            Some(TerminalCommand::DecreaseTextSize)
        }
        gtk4::gdk::Key::_0 | gtk4::gdk::Key::parenright | gtk4::gdk::Key::KP_0 => {
            Some(TerminalCommand::ResetTextSize)
        }
        _ => None,
    }
}

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
    task_chip: TaskChipWidget,
    badge: AttentionBadge,
    resume: gtk4::Button,
    decision_bar: gtk4::Box,
    decision_prompt: gtk4::Label,
    decision_options: libadwaita::WrapBox,
    decision_hint: gtk4::Label,
    shown_decision: RefCell<Option<Decision>>,
    on_action: Rc<ActionCallback>,
    pane_id: String,
    actor: IpcHandle,
    live: Cell<bool>,
    paste_cancel: RefCell<gtk4::gio::Cancellable>,
    last_size: Cell<(u16, u16)>,
    text_tenths: Cell<i32>,
    commands: gtk4::gio::SimpleActionGroup,
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
        subtitle.set_hexpand(false);
        let badge = AttentionBadge::new();
        let task_chip = TaskChipWidget::new();

        let on_action = Rc::new(cb.on_action);
        let action_button = |icon: &str, tooltip: &str, action: PaneAction| {
            let b = gtk4::Button::from_icon_name(icon);
            b.add_css_class("flat");
            b.add_css_class("pane-tool-button");
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
        let close = action_button(
            "window-close-symbolic",
            "Close Pane (Ctrl+Shift+W)",
            PaneAction::Close,
        );
        close.set_margin_start(6);
        actions.append(&close);
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

        // Title cluster (lifecycle, name, lifecycle word, task chip),
        // then attention (never recedes) and actions (on hover/focus).
        // No child of the cluster expands, so spare width stays ahead
        // of the trailing controls. Those controls keep their space
        // while hidden, and the chip stays with the title instead of
        // floating in that reserved gap.
        let info = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        info.add_css_class("pane-info");
        info.set_hexpand(true);
        info.append(&lifecycle.widget);
        info.append(&title);
        info.append(&subtitle);
        info.append(&task_chip.root);
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
        search_root.add_css_class("terminal-search");
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

        let commands = gtk4::gio::SimpleActionGroup::new();

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
            task_chip,
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
            paste_cancel: RefCell::new(gtk4::gio::Cancellable::new()),
            last_size: Cell::new((80, 24)),
            text_tenths: Cell::new(0),
            commands,
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
        // connect_local keeps the GValue string pointer. The generated
        // connect_commit converts that pointer to UTF-8 and drops a NUL.
        {
            let weak = Rc::downgrade(&w);
            w.term.connect_local("commit", false, move |values| {
                let w = weak.upgrade()?;
                let bytes = Self::commit_bytes(values)?;
                if bytes.is_empty() || !w.live.get() {
                    return None;
                }
                w.send_input(&bytes);
                None
            });
        }
        w.term.insert_action_group("term", Some(&w.commands));
        {
            for (name, command, enabled) in [
                ("copy", TerminalCommand::Copy, false),
                ("paste", TerminalCommand::Paste, true),
                (
                    "increase-text-size",
                    TerminalCommand::IncreaseTextSize,
                    true,
                ),
                (
                    "decrease-text-size",
                    TerminalCommand::DecreaseTextSize,
                    true,
                ),
                ("reset-text-size", TerminalCommand::ResetTextSize, false),
            ] {
                let action = gtk4::gio::SimpleAction::new(name, None);
                action.set_enabled(enabled);
                let weak = Rc::downgrade(&w);
                action.connect_activate(move |_, _| {
                    if let Some(w) = weak.upgrade() {
                        w.run_command(command);
                    }
                });
                w.commands.add_action(&action);
            }
            let weak = Rc::downgrade(&w);
            w.term.connect_selection_changed(move |term| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                w.set_command_enabled("copy", term.has_selection());
            });
            let weak = Rc::downgrade(&w);
            w.term.connect_increase_font_size(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.run_command(TerminalCommand::IncreaseTextSize);
                }
            });
            let weak = Rc::downgrade(&w);
            w.term.connect_decrease_font_size(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.run_command(TerminalCommand::DecreaseTextSize);
                }
            });
            let menu = gtk4::gio::Menu::new();
            menu.append(Some("Copy"), Some("term.copy"));
            menu.append(Some("Paste"), Some("term.paste"));
            menu.append(Some("Increase Text Size"), Some("term.increase-text-size"));
            menu.append(Some("Decrease Text Size"), Some("term.decrease-text-size"));
            menu.append(Some("Reset Text Size"), Some("term.reset-text-size"));
            w.term.set_context_menu_model(Some(&menu));
            // Ancestor capture runs before VTE's own key controller.
            // Only the focused terminal claims the chord, so search
            // keeps the keys it already handles.
            let keys = gtk4::EventControllerKey::new();
            keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let weak = Rc::downgrade(&w);
            keys.connect_key_pressed(move |controller, key, keycode, modifiers| {
                let unshifted = controller.current_event().and_then(|event| {
                    let event = event.downcast::<gtk4::gdk::KeyEvent>().ok()?;
                    event
                        .display()?
                        .translate_key(
                            keycode,
                            modifiers & !gtk4::gdk::ModifierType::SHIFT_MASK,
                            event.layout() as i32,
                        )
                        .map(|(key, _, _, _)| key)
                });
                let Some(command) = unshifted
                    .and_then(|key| terminal_chord(key, modifiers))
                    .or_else(|| terminal_chord(key, modifiers))
                else {
                    return gtk4::glib::Propagation::Proceed;
                };
                let Some(w) = weak.upgrade() else {
                    return gtk4::glib::Propagation::Stop;
                };
                if !w.term.has_focus() {
                    return gtk4::glib::Propagation::Proceed;
                }
                w.run_command(command);
                gtk4::glib::Propagation::Stop
            });
            w.root.add_controller(keys);
        }
        // Focus = explicit per-pane interaction: clear attention.
        {
            let weak = Rc::downgrade(&w);
            let on_focus = cb.on_focus;
            w.term.connect_has_focus_notify(move |term| {
                let Some(w) = weak.upgrade() else { return };
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
        self.term.set_cell_height_scale(CELL_HEIGHT_SCALE);
        let fg_hex = if sm.is_dark() {
            DARK_FOREGROUND
        } else {
            LIGHT_FOREGROUND
        };
        let rgba = |hex: &str| gtk4::gdk::RGBA::parse(hex).expect("palette colour");
        let inks = if sm.is_dark() {
            &DARK_PALETTE
        } else {
            &LIGHT_PALETTE
        };
        let palette: Vec<gtk4::gdk::RGBA> = inks.iter().map(|c| rgba(c)).collect();
        let palette: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        self.term.set_colors(
            Some(&rgba(fg_hex)),
            Some(&rgba(theme.pane_bg(sm.is_dark()))),
            &palette,
        );
        self.apply_text_scale();
    }

    fn apply_text_scale(&self) {
        self.term
            .set_font_scale(text_factor(self.text_tenths.get()));
    }

    fn set_text_tenths(&self, tenths: i32) {
        let tenths = tenths.clamp(TEXT_TENTH_MIN, TEXT_TENTH_MAX);
        self.text_tenths.set(tenths);
        self.apply_text_scale();
        self.set_command_enabled("increase-text-size", tenths < TEXT_TENTH_MAX);
        self.set_command_enabled("decrease-text-size", tenths > TEXT_TENTH_MIN);
        self.set_command_enabled("reset-text-size", tenths != 0);
    }

    /// Accepted prefix of a VTE `commit` emission.
    ///
    /// The signal is a `G_TYPE_STRING` plus a declared byte count. On a
    /// realized widget the native marshaller can keep that count after
    /// copying only the bytes before an embedded NUL. The prefix is the
    /// declared count when it still fits in the C string, including the
    /// terminator so Ctrl+Space (size 1, empty text) is the single NUL.
    /// A count past that storage is refused whole.
    fn commit_bytes(values: &[gtk4::glib::Value]) -> Option<Vec<u8>> {
        let declared = values.get(2)?.get::<u32>().ok()? as usize;
        let value = values.get(1)?;
        if value.type_() != gtk4::glib::Type::STRING {
            return None;
        }
        // SAFETY: a string GValue holds a NUL-terminated pointer valid for
        // this emission. The read stops at that terminator and is copied
        // before the callback returns.
        let available = unsafe {
            use gtk4::glib::translate::ToGlibPtr;
            let ptr = gtk4::glib::gobject_ffi::g_value_get_string(value.to_glib_none().0);
            if ptr.is_null() {
                Vec::new()
            } else {
                std::ffi::CStr::from_ptr(ptr).to_bytes_with_nul().to_vec()
            }
        };
        if declared > available.len() {
            None
        } else {
            Some(available[..declared].to_vec())
        }
    }

    fn set_command_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self
            .commands
            .lookup_action(name)
            .and_then(|action| action.downcast::<gtk4::gio::SimpleAction>().ok())
        {
            action.set_enabled(enabled);
        }
    }

    fn run_command(self: &Rc<Self>, command: TerminalCommand) {
        match command {
            TerminalCommand::Copy => {
                if self.term.has_selection() {
                    self.term.copy_clipboard_format(vte4::Format::Text);
                }
            }
            TerminalCommand::Paste => self.paste_from_clipboard(),
            TerminalCommand::IncreaseTextSize => {
                self.set_text_tenths(self.text_tenths.get() + 1);
            }
            TerminalCommand::DecreaseTextSize => {
                self.set_text_tenths(self.text_tenths.get() - 1);
            }
            TerminalCommand::ResetTextSize => self.set_text_tenths(0),
        }
    }

    /// The desktop clipboard answers after this call returns. A resume
    /// keeps the widget, so the reply must not paste into the next process.
    fn paste_from_clipboard(self: &Rc<Self>) {
        if !self.live.get() {
            return;
        }
        let cancel = self.paste_cancel.borrow().clone();
        let weak = Rc::downgrade(self);
        self.term
            .clipboard()
            .read_text_async(Some(&cancel.clone()), move |result| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if cancel.is_cancelled() || !pane.live.get() {
                    return;
                }
                let Ok(Some(text)) = result else {
                    return;
                };
                if text.is_empty() {
                    return;
                }
                pane.term.paste_text(text.as_str());
            });
    }

    fn send_input(&self, bytes: &[u8]) {
        use base64::Engine;
        self.send(
            "pane.input",
            json!({
                "pane_id": self.pane_id,
                "data_b64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        );
    }

    fn send(&self, method: &'static str, params: serde_json::Value) {
        let actor = self.actor.clone();
        gtk4::glib::spawn_future_local(async move {
            let _ = actor.call(method, params).await;
        });
    }

    /// Replace the visible grid with one server snapshot and keep scrollback.
    ///
    /// The snapshot has no alternate-screen identity. Leave that grid, clear
    /// origin and margins, erase each visible row, then soft-reset with
    /// DECSTR before the snapshot. VTE 0.80.1 `reset` would either drop this
    /// history or archive the previous visible rows. One feed keeps the
    /// sequence ordered without spinning the main loop.
    pub fn replace_screen(&self, data: &[u8]) {
        let rows = self.term.row_count().max(0) as usize;
        let mut batch = Vec::with_capacity(data.len() + 32 + rows * 16);
        batch.extend_from_slice(b"\x1b[?1049l\x1b[?6l\x1b[r");
        for row in 1..=rows {
            batch.extend_from_slice(b"\x1b[");
            batch.extend_from_slice(row.to_string().as_bytes());
            batch.extend_from_slice(b";1H\x1b[2K");
        }
        batch.extend_from_slice(b"\x1b[!p");
        batch.extend_from_slice(data);
        self.term.feed(&batch);
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
        self.title
            .set_text(display_title(&pane.title, &gtk4::glib::host_name()));
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
        self.prefer_chip_ellipsize();
        self.lifecycle.set(status::effective_lifecycle(pane));
        self.badge.set(pane.attention);
        status::set_attention_class(&self.root, pane.attention);

        let was_live = self.live.replace(live);
        if was_live && !live {
            self.paste_cancel
                .replace(gtk4::gio::Cancellable::new())
                .cancel();
            self.term.feed(b"\r\n\x1b[2m[process exited]\x1b[0m\r\n");
        }
        // Resume affordance for restored/resumable tombstones.
        self.resume
            .set_visible(!live && pane.agent.resume_argv.is_some());
        self.set_command_enabled("paste", live);
        self.sync_decision_bar(pane.pending_decision.as_ref());
    }

    /// Paint the worker chip in place. `None` hides it. Never touches the VTE.
    pub fn set_task_chip(&self, view: Option<&TaskChipView>) {
        self.task_chip.set(view);
        self.prefer_chip_ellipsize();
    }

    /// A short title keeps its full width while a chip is showing, so
    /// the chip's label is what ellipsizes in a narrow pane. Long
    /// titles still ellipsize. Hiding the chip restores the usual title.
    fn prefer_chip_ellipsize(&self) {
        let short_title = self.title.text().chars().count() <= 24;
        let protect = self.task_chip.root.is_visible() && short_title;
        self.title.set_ellipsize(if protect {
            gtk4::pango::EllipsizeMode::None
        } else {
            gtk4::pango::EllipsizeMode::End
        });
    }

    /// Toggle the inline decision bar. Buttons rebuild only when the
    /// decision id, answerability or options change; showing/hiding
    /// never touches the VTE.
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
                let rebuild = match self.shown_decision.borrow().as_ref() {
                    Some(shown) => {
                        shown.id != d.id
                            || shown.answerable != d.answerable
                            || shown.options != d.options
                    }
                    None => true,
                };
                if rebuild {
                    while let Some(child) = self.decision_options.first_child() {
                        self.decision_options.remove(&child);
                    }
                    let mut primary_used = false;
                    for (option_id, label) in &rendered.options {
                        let text = gtk4::Label::new(Some(label));
                        text.set_wrap(true);
                        text.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                        text.set_xalign(0.0);
                        let button = gtk4::Button::new();
                        button.set_child(Some(&text));
                        button.add_css_class("pill");
                        if !primary_used && decision_is_primary(option_id, label) {
                            button.add_css_class("suggested-action");
                            primary_used = true;
                        }
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
                    *self.shown_decision.borrow_mut() = Some(d.clone());
                }
                self.decision_bar.set_visible(true);
            }
        }
    }
}

impl Drop for PaneWidget {
    fn drop(&mut self) {
        self.paste_cancel.get_mut().cancel();
    }
}

const LIGHT_FOREGROUND: &str = "#241f31";
const DARK_FOREGROUND: &str = "#deddda";

/// GNOME palette (as in Console/Ptyxis) on dark panes.
const DARK_PALETTE: [&str; 16] = [
    "#241f31", "#c01c28", "#2ec27e", "#f5c211", "#1e78e4", "#9841bb", "#0ab9dc", "#c0bfbc",
    "#5e5c64", "#ed333b", "#57e389", "#f8e45c", "#51a1ff", "#c061cb", "#4fd2fd", "#f6f5f4",
];

/// Light panes need their own inks: GNOME's yellow and greens are made for
/// dark backgrounds and vanish on light panes (`git log` hashes, prompts).
/// Every ink, including grey output, reads at 4.5:1 on all light theme backgrounds.
const LIGHT_PALETTE: [&str; 16] = [
    "#24292f", "#cf222e", "#116329", "#7d4e00", "#0969da", "#8250df", "#1b7c83", "#656e78",
    "#57606a", "#a40e26", "#1a7f37", "#9a6700", "#196fce", "#805ac6", "#257a8e", "#69737d",
];

/// Leading for the terminal grid: VTE's default packs rows edge to edge.
const CELL_HEIGHT_SCALE: f64 = 1.1;

/// Window titles from shells read `user@host: ~/path`; the header already
/// says where the pane is, so a local `user@host:` prefix is noise. A
/// different host (an SSH session) is information and stays.
fn display_title<'a>(title: &'a str, local_host: &str) -> &'a str {
    let trimmed = title.trim();
    let short = |host: &str| host.split('.').next().unwrap_or(host).to_ascii_lowercase();
    // `\u@\h: \w` and the space-less `\u@\h:\w` (only before a path).
    let parts = trimmed.split_once(": ").or_else(|| {
        trimmed
            .split_once(':')
            .filter(|(_, rest)| rest.starts_with(['~', '/']))
    });
    match parts {
        Some((who, rest))
            if !rest.is_empty()
                && !who.contains(char::is_whitespace)
                && who.split_once('@').is_some_and(|(user, host)| {
                    !user.is_empty()
                        && !host.is_empty()
                        && (host.eq_ignore_ascii_case(local_host)
                            || ((!host.contains('.') || !local_host.contains('.'))
                                && short(host) == short(local_host)))
                }) =>
        {
            rest
        }
        _ => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_titles_drop_the_user_at_host_prefix() {
        let t = |title| display_title(title, "luxhole");
        assert_eq!(t("simone@luxhole: ~/code/app"), "~/code/app");
        assert_eq!(t("root@LuxHole.local: /"), "/", "short, case-blind host");
        assert_eq!(
            t("deploy@prod-1: /srv"),
            "deploy@prod-1: /srv",
            "a remote host is kept"
        );
        assert_eq!(t("claude"), "claude");
        assert_eq!(t("vim: main.rs"), "vim: main.rs", "no @, kept");
        assert_eq!(t("Note: a@b c"), "Note: a@b c", "@ after the colon");
        assert_eq!(
            t("a b@luxhole: x"),
            "a b@luxhole: x",
            "spaces before the colon"
        );
        assert_eq!(t("me@luxhole: "), "me@luxhole: ", "nothing after it");
        assert_eq!(t("me@luxhole:~/src"), "~/src", "no space before a path");
        assert_eq!(t("me@luxhole:x"), "me@luxhole:x", "no space, not a path");
        assert_eq!(t("  vim: main.rs  "), "  vim: main.rs  ");
        assert_eq!(
            display_title(
                "deploy@build.production.example: /srv",
                "build.office.example"
            ),
            "deploy@build.production.example: /srv",
            "distinct qualified hosts keep their identity"
        );
        assert_eq!(
            display_title("me@BUILD.office.example: /src", "build.office.example"),
            "/src"
        );
        assert_eq!(
            display_title("me@build: /src", "build.office.example"),
            "/src"
        );
    }

    /// WCAG relative luminance for an opaque sRGB colour.
    fn luminance(hex: &str) -> f64 {
        let channel = |i: usize| {
            let v = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    #[test]
    fn light_palette_inks_read_on_every_theme() {
        for theme in Theme::ALL {
            let background = luminance(theme.pane_bg(false));
            for (i, hex) in LIGHT_PALETTE.iter().enumerate() {
                let ratio = (background + 0.05) / (luminance(hex) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "{theme:?} colour {i} {hex}: {ratio:.2}:1 < 4.5"
                );
            }
        }
    }
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
    fn dropped_pane_widget_is_freed() {
        libadwaita::init().unwrap();
        let (actor, _requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "freed",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let weak = Rc::downgrade(&pane);
        drop(pane);
        assert!(
            weak.upgrade().is_none(),
            "terminal signal handlers keep the pane alive"
        );
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
    fn allow_once_is_the_only_primary_answer() {
        assert!(decision_is_primary("once", "Allow once"));
        assert!(decision_is_primary("once", "Once"));
        assert!(decision_is_primary("other", "Allow once"));
        assert!(!decision_is_primary("always", "Allow for this session"));
        assert!(!decision_is_primary("session", "Allow for this session"));
        assert!(!decision_is_primary("deny", "Deny"));
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
        approval.prompt = "Updated approval context".into();
        approval.received_at = chrono::Utc::now();
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        assert_eq!(
            widget.decision_options.first_child().unwrap(),
            choices[0].clone().upcast::<gtk4::Widget>(),
            "prompt and timestamp changes must retain the choice widgets"
        );
        approval.answerable = false;
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        assert!(widget.decision_hint.is_visible());
        assert!(
            widget.decision_options.first_child().is_none(),
            "the same decision ID becoming read-only must remove stale choices"
        );
        approval.answerable = true;
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        assert!(!widget.decision_hint.is_visible());
        assert!(widget.decision_options.first_child().is_some());
        approval.options[0].label = "Allow current request".into();
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        let renamed = widget
            .decision_options
            .first_child()
            .unwrap()
            .downcast::<gtk4::Button>()
            .unwrap();
        assert_eq!(
            renamed
                .child()
                .unwrap()
                .downcast::<gtk4::Label>()
                .unwrap()
                .text(),
            "Allow current request",
            "a label-only update under the same decision ID must refresh choices"
        );
        approval.options[0].id = "allow_current".into();
        pane.pending_decision = Some(approval.clone());
        widget.update_meta(&pane);
        let current = widget
            .decision_options
            .first_child()
            .unwrap()
            .downcast::<gtk4::Button>()
            .unwrap();
        assert_eq!(
            current
                .child()
                .unwrap()
                .downcast::<gtk4::Label>()
                .unwrap()
                .text(),
            "Allow current request"
        );
        current.emit_clicked();
        assert!(matches!(
            selected.borrow().as_ref(),
            Some((id, PaneAction::AnswerDecision { decision_id, option_id }))
                if id == "pane-narrow" && decision_id == "d1" && option_id == "allow_current"
        ));
        assert_eq!(widget.term, terminal);
        let capture = |name: &str| {
            let Some(directory) = std::env::var_os("SIGNALTTY_UI_EVIDENCE") else {
                return;
            };
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(120);
            while std::time::Instant::now() < deadline {
                while gtk4::glib::MainContext::default().iteration(false) {}
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let paintable = gtk4::WidgetPaintable::new(Some(&window));
            let snapshot = gtk4::Snapshot::new();
            paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
            let renderer = gtk4::gsk::CairoRenderer::new();
            renderer.realize_for_display(&display).unwrap();
            let texture = renderer.render_texture(snapshot.to_node().unwrap(), None);
            renderer.unrealize();
            texture
                .save_to_png(directory.join(format!("{name}.png")))
                .unwrap();
        };
        let style = libadwaita::StyleManager::default();
        let old_scheme = style.color_scheme();
        for (name, scheme) in [
            ("light", libadwaita::ColorScheme::ForceLight),
            ("dark", libadwaita::ColorScheme::ForceDark),
        ] {
            style.set_color_scheme(scheme);
            if scheme == libadwaita::ColorScheme::ForceDark {
                window.add_css_class("dark");
            } else {
                window.remove_css_class("dark");
            }
            approval.answerable = true;
            pane.pending_decision = Some(approval.clone());
            widget.update_meta(&pane);
            capture(&format!("approval-current-{name}"));
            approval.options[0].id = "once".into();
            pane.pending_decision = Some(approval.clone());
            widget.update_meta(&pane);
            capture(&format!("approval-primary-{name}"));
            approval.options[0].id = "allow_current".into();
            approval.answerable = false;
            pane.pending_decision = Some(approval.clone());
            widget.update_meta(&pane);
            assert!(widget.decision_options.first_child().is_none());
            capture(&format!("approval-read-only-{name}"));
            assert_eq!(widget.term, terminal);
        }
        style.set_color_scheme(old_scheme);
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
        let split_down = control.next_sibling().unwrap();
        let close = actions.last_child().unwrap();
        let split_bounds = split_down.compute_bounds(&pane.root).unwrap();
        let close_bounds = close.compute_bounds(&pane.root).unwrap();
        assert!(
            close_bounds.x() - split_bounds.x() - split_bounds.width() >= 8.0,
            "close must be visually separated from the split pair"
        );
        assert!(close.width() >= 28 && close.height() >= 28);
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

    fn pump(ms: u64) {
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            while gtk4::glib::MainContext::default().pending() {
                gtk4::glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn pump_until(mut done: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !done() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for the terminal"
            );
            while gtk4::glib::MainContext::default().pending() {
                gtk4::glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn present_pane(pane: &PaneWidget) -> gtk4::Window {
        let window = gtk4::Window::new();
        window.set_default_size(720, 420);
        window.set_child(Some(&pane.root));
        window.present();
        pump_until(|| pane.term.is_realized() && pane.term.row_count() > 0);
        pane.focus();
        window
    }

    fn take_inputs(
        requests: &mut tokio::sync::mpsc::UnboundedReceiver<crate::actor::ActorRequest>,
        wait_for_payload: bool,
    ) -> Vec<Vec<u8>> {
        use base64::Engine;
        let mut inputs = Vec::new();
        let deadline = std::time::Instant::now()
            + if wait_for_payload {
                std::time::Duration::from_secs(1)
            } else {
                std::time::Duration::from_millis(250)
            };
        let mut quiet = std::time::Instant::now();
        while std::time::Instant::now() < deadline {
            while gtk4::glib::MainContext::default().pending() {
                gtk4::glib::MainContext::default().iteration(false);
            }
            let mut saw = false;
            while let Ok(request) = requests.try_recv() {
                saw = true;
                if let crate::actor::ActorRequest::Call {
                    method,
                    params,
                    reply,
                    ..
                } = request
                {
                    if method == "pane.input" {
                        let encoded = params
                            .get("data_b64")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        inputs.push(
                            base64::engine::general_purpose::STANDARD
                                .decode(encoded)
                                .unwrap_or_default(),
                        );
                    }
                    let _ = reply.send(Ok(serde_json::json!({})));
                }
            }
            if saw {
                quiet = std::time::Instant::now();
            } else if !inputs.is_empty() && quiet.elapsed() > std::time::Duration::from_millis(40) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        inputs
    }

    fn sample_pane(live: LiveState) -> Pane {
        let mut pane = Pane::new(
            "ws".into(),
            "tab".into(),
            "/tmp".into(),
            vec!["shell".into()],
            signaltty_core::PtySize::new(80, 24).unwrap(),
            chrono::Utc::now(),
        );
        pane.live = live;
        pane
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn realized_terminal_forwards_commit_bytes_and_refuses_truncated_nul() {
        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "input",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let window = present_pane(&pane);
        let _ = take_inputs(&mut requests, false);

        pane.term.feed_child(&[0]);
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![vec![0]],
            "Ctrl+Space must deliver one NUL"
        );

        pane.term.feed_child(&[0x03]);
        assert_eq!(take_inputs(&mut requests, true), vec![vec![0x03]]);

        pane.term.feed_child("héllo".as_bytes());
        assert_eq!(
            take_inputs(&mut requests, true),
            vec!["héllo".as_bytes().to_vec()]
        );

        pane.term.feed_child(b"one\ntwo");
        assert_eq!(take_inputs(&mut requests, true), vec![b"one\ntwo".to_vec()]);

        window.clipboard().set_text("α\nβ");
        pane.term.paste_clipboard();
        assert_eq!(
            take_inputs(&mut requests, true),
            vec!["α\rβ".as_bytes().to_vec()],
            "native paste maps the newline to CR and adds no further Enter"
        );

        pane.term.feed(b"\x1b[?2004h");
        pump(80);
        window.clipboard().set_text("paste");
        pane.term.paste_clipboard();
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"\x1b[200~paste\x1b[201~".to_vec()]
        );

        pane.term.feed_child(&[b'a', 0, b'b']);
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "a realized terminal must refuse an already-truncated embedded NUL"
        );

        pane.update_meta(&sample_pane(LiveState::Exited { code: None }));
        pane.term.feed_child(b"x");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        window.close();
    }

    fn activate_paste(pane: &PaneWidget) {
        pane.commands
            .lookup_action("paste")
            .expect("paste action")
            .activate(None);
    }

    fn pending_inputs(
        requests: &mut tokio::sync::mpsc::UnboundedReceiver<crate::actor::ActorRequest>,
    ) -> Vec<Vec<u8>> {
        use base64::Engine;
        let mut inputs = Vec::new();
        while let Ok(request) = requests.try_recv() {
            if let crate::actor::ActorRequest::Call {
                method,
                params,
                reply,
                ..
            } = request
            {
                if method == "pane.input" {
                    let encoded = params
                        .get("data_b64")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    inputs.push(
                        base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .unwrap_or_default(),
                    );
                }
                let _ = reply.send(Ok(serde_json::json!({})));
            }
        }
        inputs
    }

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn paste_started_before_exit_is_not_delivered_after_resume() {
        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "paste-race",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let window = present_pane(&pane);
        let _ = take_inputs(&mut requests, false);

        window.clipboard().set_text("α\nβ");
        activate_paste(&pane);
        assert_eq!(
            take_inputs(&mut requests, true),
            vec!["α\rβ".as_bytes().to_vec()],
            "paste keeps the newline as CR and adds no Enter"
        );

        activate_paste(&pane);
        activate_paste(&pane);
        assert_eq!(
            take_inputs(&mut requests, true),
            vec!["α\rβ".as_bytes().to_vec(); 2],
            "each paste command in the same live process must be delivered"
        );

        pane.term.feed(b"\x1b[?2004h");
        pump(80);
        window.clipboard().set_text("one\ntwo");
        activate_paste(&pane);
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"\x1b[200~one\rtwo\x1b[201~".to_vec()],
            "paste keeps bracketed mode"
        );

        window.clipboard().set_text("STALE");
        activate_paste(&pane);
        assert_eq!(
            pending_inputs(&mut requests),
            Vec::<Vec<u8>>::new(),
            "the clipboard read must still be in flight"
        );
        pane.update_meta(&sample_pane(LiveState::Exited { code: None }));
        pane.update_meta(&sample_pane(LiveState::Live));
        assert_eq!(
            take_inputs(&mut requests, true),
            Vec::<Vec<u8>>::new(),
            "stale clipboard text must not reach the resumed process"
        );

        window.clipboard().set_text("AFTER");
        activate_paste(&pane);
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"\x1b[200~AFTER\x1b[201~".to_vec()],
            "a paste started after resume still reaches that process"
        );

        window.clipboard().set_text("GONE");
        activate_paste(&pane);
        assert_eq!(pending_inputs(&mut requests), Vec::<Vec<u8>>::new());
        pane.update_meta(&sample_pane(LiveState::Exited { code: None }));
        assert_eq!(
            take_inputs(&mut requests, true),
            Vec::<Vec<u8>>::new(),
            "an in-flight paste is dropped when the pane stays exited"
        );
        window.close();

        let (actor, mut dropped_requests) = IpcHandle::test_channel();
        let dropped = PaneWidget::new(
            "paste-dropped",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let weak = Rc::downgrade(&dropped);
        let dropped_window = present_pane(&dropped);
        let _ = take_inputs(&mut dropped_requests, false);
        dropped_window.clipboard().set_text("DROPPED");
        activate_paste(&dropped);
        assert_eq!(pending_inputs(&mut dropped_requests), Vec::<Vec<u8>>::new());
        drop(dropped_window);
        drop(dropped);
        assert!(
            weak.upgrade().is_none(),
            "an in-flight paste must not keep the pane"
        );
        assert_eq!(
            take_inputs(&mut dropped_requests, true),
            Vec::<Vec<u8>>::new(),
            "a dropped pane must not paste"
        );
    }

    fn press_chord(title: &str, chord: &str) {
        let output = std::process::Command::new("xdotool")
            .args([
                "search",
                "--sync",
                "--onlyvisible",
                "--name",
                title,
                "windowfocus",
                "--sync",
                "key",
                "--window",
                "%1",
                "--clearmodifiers",
                chord,
            ])
            .output()
            .unwrap_or_else(|error| panic!("xdotool {chord}: {error}"));
        assert!(
            output.status.success(),
            "xdotool {chord}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        pump(200);
    }

    fn clipboard_text(widget: &impl gtk4::prelude::WidgetExt) -> String {
        let (sender, receiver) = std::sync::mpsc::channel();
        widget
            .clipboard()
            .read_text_async(None::<&gtk4::gio::Cancellable>, move |result| {
                let text = result
                    .ok()
                    .flatten()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let _ = sender.send(text);
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            while gtk4::glib::MainContext::default().pending() {
                gtk4::glib::MainContext::default().iteration(false);
            }
            if let Ok(text) = receiver.try_recv() {
                return text;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "clipboard read timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    #[ignore = "requires a GTK display and xdotool; run with dbus-run-session"]
    fn terminal_chords_copy_paste_and_stay_consumed_when_unavailable() {
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "keys",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let title = format!("signaltty-vte-keys-{}", std::process::id());
        let window = present_pane(&pane);
        window.set_title(Some(&title));
        pump(100);
        let _ = take_inputs(&mut requests, false);

        window.clipboard().set_text("KEEP");
        pump(50);
        press_chord(&title, "ctrl+shift+c");
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "Copy with no selection must not reach the child as Ctrl+C"
        );
        assert_eq!(clipboard_text(&window), "KEEP");

        pane.term.feed(b"COPY_TOKEN");
        pump(80);
        pane.term.select_all();
        pump(40);
        press_chord(&title, "ctrl+shift+c");
        assert!(
            clipboard_text(&window).contains("COPY_TOKEN"),
            "clipboard was {:?}",
            clipboard_text(&window)
        );
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());

        window.clipboard().set_text("PASTE_TOKEN");
        pump(40);
        press_chord(&title, "ctrl+shift+v");
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"PASTE_TOKEN".to_vec()]
        );

        pane.term.feed(b"\x1b[?2004h");
        pump(80);
        press_chord(&title, "ctrl+shift+v");
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"\x1b[200~PASTE_TOKEN\x1b[201~".to_vec()]
        );

        pane.update_meta(&sample_pane(LiveState::Exited { code: None }));
        press_chord(&title, "ctrl+shift+v");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());

        pane.update_meta(&sample_pane(LiveState::Live));
        press_chord(&title, "ctrl+c");
        assert_eq!(take_inputs(&mut requests, true), vec![vec![0x03]]);
        press_chord(&title, "ctrl+v");
        assert_eq!(take_inputs(&mut requests, true), vec![vec![0x16]]);

        pane.present_search();
        pump(40);
        press_chord(&title, "ctrl+shift+c");
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "search keeps its own key handling"
        );

        pane.close_search();
        assert!(pane.term.grab_focus());
        pump(40);
        let _ = take_inputs(&mut requests, false);
        press_chord(&title, "Menu");
        assert!(
            menu_is_open(&window),
            "Menu opens the terminal context menu"
        );
        press_chord(&title, "Escape");
        assert!(
            !menu_is_open(&window),
            "Escape dismisses the terminal context menu"
        );
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "Menu and Escape must not reach the child"
        );
        press_chord(&title, "shift+F10");
        assert!(
            menu_is_open(&window),
            "Shift+F10 opens the terminal context menu"
        );
        press_chord(&title, "Escape");
        assert!(
            !menu_is_open(&window),
            "Escape dismisses the Shift+F10 menu"
        );
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "Shift+F10 and Escape must not reach the child"
        );

        let model = pane
            .term
            .context_menu_model()
            .expect("terminal context menu");
        let labels: Vec<String> = (0..model.n_items())
            .filter_map(|index| {
                model
                    .item_attribute_value(index, "label", None)
                    .and_then(|value| value.str().map(str::to_string))
            })
            .collect();
        assert!(labels.iter().any(|label| label == "Copy"));
        assert!(labels.iter().any(|label| label == "Paste"));
        window.close();
    }

    fn terminal_contents(term: &vte4::Terminal) -> String {
        let stream = gtk4::gio::MemoryOutputStream::new_resizable();
        term.write_contents_sync(
            &stream,
            vte4::WriteFlags::Default,
            None::<&gtk4::gio::Cancellable>,
        )
        .expect("terminal contents");
        stream
            .close(None::<&gtk4::gio::Cancellable>)
            .expect("close contents");
        String::from_utf8_lossy(&stream.steal_as_bytes()).into_owned()
    }

    fn visible_text(term: &vte4::Terminal) -> String {
        term.text_format(vte4::Format::Text)
            .map(|text| text.to_string())
            .unwrap_or_default()
    }

    fn click_window(title: &str, x: i32, y: i32, button: &str) {
        let listed = std::process::Command::new("xdotool")
            .args(["search", "--sync", "--onlyvisible", "--name", title])
            .output()
            .unwrap_or_else(|error| panic!("xdotool search: {error}"));
        assert!(
            listed.status.success(),
            "xdotool search: {}",
            String::from_utf8_lossy(&listed.stderr)
        );
        let id = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        assert!(!id.is_empty(), "xdotool found no window {title}");
        let focused = std::process::Command::new("xdotool")
            .args(["windowfocus", "--sync", &id])
            .status()
            .unwrap_or_else(|error| panic!("xdotool focus: {error}"));
        assert!(focused.success(), "xdotool focus failed");
        let geometry = std::process::Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", &id])
            .output()
            .unwrap_or_else(|error| panic!("xdotool geometry: {error}"));
        assert!(geometry.status.success());
        let mut origin_x = 0;
        let mut origin_y = 0;
        for line in String::from_utf8_lossy(&geometry.stdout).lines() {
            if let Some(value) = line.strip_prefix("X=") {
                origin_x = value.parse().expect("window x");
            } else if let Some(value) = line.strip_prefix("Y=") {
                origin_y = value.parse().expect("window y");
            }
        }
        // XTEST, not XSendEvent: GTK discards send_event button clicks.
        let output = std::process::Command::new("xdotool")
            .args([
                "mousemove",
                &(origin_x + x).to_string(),
                &(origin_y + y).to_string(),
                "click",
                button,
            ])
            .output()
            .unwrap_or_else(|error| panic!("xdotool click: {error}"));
        assert!(
            output.status.success(),
            "xdotool click: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        pump(200);
    }

    #[test]
    #[ignore = "requires a GTK display and xdotool; run with dbus-run-session"]
    fn replace_screen_keeps_one_history_marker_and_restores_modes() {
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "screen",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let title = format!("signaltty-vte-screen-{}", std::process::id());
        let window = present_pane(&pane);
        window.set_title(Some(&title));
        let _ = take_inputs(&mut requests, false);

        pane.term.feed(b"HISTORY_MARKER\r\n");
        let extra = pane.term.row_count().max(1) as usize + 4;
        for index in 0..extra {
            pane.term.feed(format!("filler-{index:02}\r\n").as_bytes());
        }
        pane.term.feed(b"OLD_VISIBLE");
        pump(80);
        let before = terminal_contents(&pane.term);
        assert_eq!(
            before.matches("HISTORY_MARKER").count(),
            1,
            "history must be seeded once before reconnect"
        );

        let snapshot = b"\x1b[0m\x1b[H\x1b[JMAIN_CANON\x1b[?2004h\x1b[?1000h\x1b[?1006h";
        pane.replace_screen(snapshot);
        pane.feed(b"LIVE_TAIL");
        pump(80);
        assert!(
            visible_text(&pane.term).contains("MAIN_CANON"),
            "queued live output follows the restored screen"
        );
        assert!(visible_text(&pane.term).contains("LIVE_TAIL"));
        pane.replace_screen(snapshot);
        pump(80);

        let all = terminal_contents(&pane.term);
        let visible = visible_text(&pane.term);
        assert_eq!(
            all.matches("HISTORY_MARKER").count(),
            1,
            "two reconnects keep the earlier line once: {all}"
        );
        assert_eq!(all.matches("MAIN_CANON").count(), 1, "{all}");
        assert_eq!(visible.matches("MAIN_CANON").count(), 1, "{visible}");
        assert_eq!(all.matches("OLD_VISIBLE").count(), 0, "{all}");
        assert_eq!(
            all.matches("LIVE_TAIL").count(),
            0,
            "the next snapshot replaces queued live bytes without archiving them: {all}"
        );

        window.clipboard().set_text("paste");
        pane.term.paste_clipboard();
        assert_eq!(
            take_inputs(&mut requests, true),
            vec![b"\x1b[200~paste\x1b[201~".to_vec()],
            "the snapshot restores bracketed paste after DECSTR"
        );
        let point = pane
            .term
            .compute_point(
                &window,
                &gtk4::graphene::Point::new(
                    pane.term.width() as f32 / 2.0,
                    pane.term.height() as f32 / 2.0,
                ),
            )
            .expect("terminal coordinates");
        click_window(
            &title,
            point.x().round() as i32,
            point.y().round() as i32,
            "1",
        );
        let reports = take_inputs(&mut requests, true);
        let blob: Vec<u8> = reports.into_iter().flatten().collect();
        assert!(
            blob.windows(3).any(|seq| seq == b"\x1b[<"),
            "the snapshot restores SGR mouse tracking: {blob:?}"
        );
        window.close();

        let (actor, _requests) = IpcHandle::test_channel();
        let alt = PaneWidget::new(
            "alt",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let alt_window = present_pane(&alt);
        alt.term.feed(b"HISTORY_MARKER\r\n");
        let extra = alt.term.row_count().max(1) as usize + 4;
        for index in 0..extra {
            alt.term.feed(format!("filler-{index:02}\r\n").as_bytes());
        }
        alt.term.feed(b"MAIN_VISIBLE\x1b[?1049h\x1b[HALT_OLD");
        pump(80);
        let alt_snapshot = b"\x1b[0m\x1b[H\x1b[JALT_CANON";
        alt.replace_screen(alt_snapshot);
        alt.replace_screen(alt_snapshot);
        pump(80);
        let all = terminal_contents(&alt.term);
        let visible = visible_text(&alt.term);
        assert_eq!(all.matches("HISTORY_MARKER").count(), 1, "{all}");
        assert_eq!(all.matches("ALT_CANON").count(), 1, "{all}");
        assert_eq!(visible.matches("ALT_CANON").count(), 1, "{visible}");
        assert_eq!(all.matches("ALT_OLD").count(), 0, "{all}");
        alt_window.close();
    }

    fn assert_scale(term: &vte4::Terminal, expected: f64) {
        let actual = term.font_scale();
        assert!(
            (actual - expected).abs() < 0.001,
            "font scale {actual} != {expected}"
        );
    }

    fn command_enabled(pane: &PaneWidget, name: &str) -> bool {
        pane.commands.lookup_action(name).expect(name).is_enabled()
    }

    #[test]
    #[ignore = "requires Xvfb, xdotool and setxkbmap"]
    fn terminal_text_scale_uses_unshifted_italian_layout_keys() {
        const CHILD_FLAG: &str = "SIGNALTTY_TEST_IT_KEYMAP_CHILD";
        const TEST_NAME: &str =
            "terminal::tests::terminal_text_scale_uses_unshifted_italian_layout_keys";
        if std::env::var_os(CHILD_FLAG).is_none() {
            let output = std::process::Command::new("xvfb-run")
                .args([
                    "-a",
                    "-s",
                    "-screen 0 1600x1200x24 -noreset",
                    "dbus-run-session",
                    "--",
                    "sh",
                    "-c",
                    "setxkbmap it && setxkbmap -query && exec \"$1\" \"$2\" --exact --ignored --test-threads=1",
                    "sh",
                ])
                .arg(std::env::current_exe().unwrap())
                .arg(TEST_NAME)
                .env(CHILD_FLAG, "1")
                .env("GDK_BACKEND", "x11")
                .env_remove("WAYLAND_DISPLAY")
                .output()
                .expect("isolated Italian keymap test");
            assert!(
                output.status.success(),
                "Italian keymap child failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "zoom-it",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let title = format!("signaltty-vte-zoom-it-{}", std::process::id());
        let window = present_pane(&pane);
        window.set_title(Some(&title));
        let _ = take_inputs(&mut requests, false);

        for (chord, scale) in [
            ("ctrl+shift+plus", 1.1),
            ("ctrl+shift+minus", 1.0),
            ("ctrl+shift+plus", 1.1),
            ("ctrl+shift+0", 1.0),
        ] {
            press_chord(&title, chord);
            assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
            assert_scale(&pane.term, scale);
        }
        window.close();
    }

    #[test]
    #[ignore = "requires a GTK display and xdotool; run with dbus-run-session"]
    fn terminal_text_scale_follows_real_chords_and_stays_per_pane() {
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        libadwaita::init().unwrap();
        let (actor, mut requests) = IpcHandle::test_channel();
        let pane = PaneWidget::new(
            "zoom-a",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let (other_actor, mut other_requests) = IpcHandle::test_channel();
        let other = PaneWidget::new(
            "zoom-b",
            other_actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let title = format!("signaltty-vte-zoom-a-{}", std::process::id());
        let window = present_pane(&pane);
        window.set_title(Some(&title));
        let other_window = present_pane(&other);
        other_window.set_title(Some(&format!(
            "signaltty-vte-zoom-b-{}",
            std::process::id()
        )));
        let same_term = pane.term.clone();
        let face = pane.term.font().expect("desktop font").to_string();
        let _ = take_inputs(&mut requests, false);
        let _ = take_inputs(&mut other_requests, false);
        assert_scale(&pane.term, 1.0);
        assert_scale(&other.term, 1.0);

        press_chord(&title, "ctrl+shift+plus");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.1);
        assert_scale(&other.term, 1.0);

        press_chord(&title, "ctrl+shift+equal");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.2);
        pane.apply_style(signaltty_core::Theme::Signal);
        assert_scale(&pane.term, 1.2);
        assert!(
            (pane.term.cell_height_scale() - 1.1).abs() < 0.001,
            "leading stays on the desktop cell height"
        );
        assert_eq!(pane.term.font().expect("font").to_string(), face);

        press_chord(&title, "ctrl+shift+minus");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.1);
        press_chord(&title, "ctrl+shift+underscore");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.0);

        press_chord(&title, "ctrl+shift+minus");
        assert_scale(&pane.term, 0.9);
        press_chord(&title, "ctrl+shift+0");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.0);

        press_chord(&title, "ctrl+shift+equal");
        assert_scale(&pane.term, 1.1);
        press_chord(&title, "ctrl+shift+parenright");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.0);

        press_chord(&title, "ctrl+shift+KP_Add");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 1.1);
        press_chord(&title, "ctrl+shift+KP_Subtract");
        assert_scale(&pane.term, 1.0);
        press_chord(&title, "ctrl+shift+KP_0");
        assert_scale(&pane.term, 1.0);

        for _ in 0..12 {
            press_chord(&title, "ctrl+shift+plus");
        }
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 2.0);
        assert!(!command_enabled(&pane, "increase-text-size"));
        press_chord(&title, "ctrl+shift+plus");
        assert_eq!(
            take_inputs(&mut requests, false),
            Vec::<Vec<u8>>::new(),
            "zoom at the upper bound must not reach the child"
        );
        assert_scale(&pane.term, 2.0);

        for _ in 0..20 {
            press_chord(&title, "ctrl+shift+minus");
        }
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 0.5);
        assert!(!command_enabled(&pane, "decrease-text-size"));
        press_chord(&title, "ctrl+shift+minus");
        assert_eq!(take_inputs(&mut requests, false), Vec::<Vec<u8>>::new());
        assert_scale(&pane.term, 0.5);

        press_chord(&title, "ctrl+shift+0");
        assert_scale(&pane.term, 1.0);
        assert!(!command_enabled(&pane, "reset-text-size"));
        assert!(command_enabled(&pane, "increase-text-size"));
        assert!(command_enabled(&pane, "decrease-text-size"));
        assert_scale(&other.term, 1.0);
        assert_eq!(pane.term, same_term);
        assert_ne!(pane.term, other.term);
        pane.present_search();
        assert_eq!(pane.term, same_term);

        let model = pane.term.context_menu_model().expect("menu");
        let labels: Vec<String> = (0..model.n_items())
            .filter_map(|index| {
                model
                    .item_attribute_value(index, "label", None)
                    .and_then(|value| value.str().map(str::to_string))
            })
            .collect();
        assert!(labels.iter().any(|label| label == "Increase Text Size"));
        assert!(labels.iter().any(|label| label == "Decrease Text Size"));
        assert!(labels.iter().any(|label| label == "Reset Text Size"));
        window.close();
        other_window.close();
    }

    fn visible_popover(widget: &impl IsA<gtk4::Widget>) -> bool {
        let widget = widget.as_ref();
        if widget.is_visible() && widget.downcast_ref::<gtk4::Popover>().is_some() {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            if visible_popover(&next) {
                return true;
            }
            child = next.next_sibling();
        }
        false
    }

    fn menu_is_open(window: &gtk4::Window) -> bool {
        visible_popover(window) || gtk4::Window::list_toplevels().iter().any(visible_popover)
    }

    fn popdown_popovers(widget: &impl IsA<gtk4::Widget>) {
        let widget = widget.as_ref();
        if let Some(popover) = widget.downcast_ref::<gtk4::Popover>() {
            popover.popdown();
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            popdown_popovers(&next);
            child = next.next_sibling();
        }
    }

    fn capture_window(window: &gtk4::Window, path: &std::path::Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(120);
        while std::time::Instant::now() < deadline {
            while gtk4::glib::MainContext::default().iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let paintable = gtk4::WidgetPaintable::new(Some(window));
        let snapshot = gtk4::Snapshot::new();
        paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
        let renderer = gtk4::gsk::CairoRenderer::new();
        renderer
            .realize_for_display(&gtk4::prelude::WidgetExt::display(window))
            .unwrap();
        let texture = renderer.render_texture(snapshot.to_node().unwrap(), None);
        renderer.unrealize();
        texture.save_to_png(path).unwrap();
    }

    #[test]
    #[ignore = "requires a GTK display and xdotool; run with dbus-run-session"]
    fn terminal_scale_and_menu_render_light_and_dark() {
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        libadwaita::init().unwrap();
        let (actor, _) = IpcHandle::test_channel();
        let large = PaneWidget::new(
            "large",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        let (actor, _) = IpcHandle::test_channel();
        let small = PaneWidget::new(
            "small",
            actor,
            PaneCallbacks {
                on_focus: Box::new(|_| {}),
                on_action: Box::new(|_, _| {}),
            },
        );
        large.set_text_tenths(10);
        small.set_text_tenths(-5);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.append(&large.root);
        row.append(&small.root);
        let window = gtk4::Window::new();
        let title = format!("signaltty-vte-evidence-{}", std::process::id());
        window.set_title(Some(&title));
        window.set_default_size(960, 420);
        window.set_child(Some(&row));
        window.present();
        pump_until(|| {
            large.term.is_realized()
                && small.term.is_realized()
                && large.term.row_count() > 0
                && small.term.row_count() > 0
        });
        let style = libadwaita::StyleManager::default();
        let old_scheme = style.color_scheme();
        let directory = std::env::var_os("SIGNALTTY_UI_EVIDENCE").map(std::path::PathBuf::from);
        if let Some(directory) = &directory {
            std::fs::create_dir_all(directory).unwrap();
        }
        for (name, scheme) in [
            ("light", libadwaita::ColorScheme::ForceLight),
            ("dark", libadwaita::ColorScheme::ForceDark),
        ] {
            for top in gtk4::Window::list_toplevels() {
                popdown_popovers(&top);
            }
            pump(40);
            style.set_color_scheme(scheme);
            if scheme == libadwaita::ColorScheme::ForceDark {
                window.add_css_class("dark");
            } else {
                window.remove_css_class("dark");
            }
            large.apply_style(signaltty_core::Theme::Signal);
            small.apply_style(signaltty_core::Theme::Signal);
            large.term.feed(b"\x1b[HLARGE TEXT\r\n");
            small.term.feed(b"\x1b[Hsmall text\r\n");
            pump(80);
            large.term.select_all();
            pump(40);
            assert!(
                large.term.has_selection() && command_enabled(&large, "copy"),
                "selection={} copy={}",
                large.term.has_selection(),
                command_enabled(&large, "copy")
            );
            assert!(!command_enabled(&large, "increase-text-size"));
            large.focus();
            let point = large
                .term
                .compute_point(
                    &window,
                    &gtk4::graphene::Point::new(
                        large.term.width() as f32 / 2.0,
                        large.term.height() as f32 / 2.0,
                    ),
                )
                .expect("terminal coordinates");
            click_window(
                &title,
                point.x().round() as i32,
                point.y().round() as i32,
                "3",
            );
            if let Some(directory) = &directory {
                capture_window(
                    &window,
                    &directory.join(format!("terminal-scale-{name}.png")),
                );
                let root = directory.join(format!("terminal-scale-menu-{name}.png"));
                let status = std::process::Command::new("import")
                    .args(["-window", "root", root.to_str().unwrap()])
                    .status()
                    .expect("import");
                assert!(status.success(), "root capture failed");
            }
            assert!(menu_is_open(&window), "right click shows the terminal menu");
            pump(40);
        }
        style.set_color_scheme(old_scheme);
        window.close();
    }
}

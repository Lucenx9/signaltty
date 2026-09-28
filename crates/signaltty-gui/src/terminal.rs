//! One pane: VTE terminal (fed externally from the server stream —
//! the PTY lives in the server, never here) inside an attention-ring
//! frame, with a compact status header.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::json;
use vte4::TerminalExt;

use signaltty_core::model::{LiveState, Pane};

use crate::actor::IpcHandle;
use crate::util::attention_css;

pub struct PaneCallbacks {
    pub on_focus: Box<dyn Fn(&str)>,
    pub on_resume: Box<dyn Fn(&str)>,
}

pub struct PaneWidget {
    pub frame: gtk4::Frame,
    term: vte4::Terminal,
    title_label: gtk4::Label,
    meta_label: gtk4::Label,
    resume_button: gtk4::Button,
    pane_id: String,
    actor: IpcHandle,
    live: Cell<bool>,
    last_size: Cell<(u16, u16)>,
}

impl PaneWidget {
    pub fn new(pane_id: &str, actor: IpcHandle, cb: PaneCallbacks) -> Rc<PaneWidget> {
        let frame = gtk4::Frame::new(None);
        frame.add_css_class("pane-frame");
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header.set_margin_start(6);
        header.set_margin_end(6);
        let title_label = gtk4::Label::new(None);
        title_label.set_xalign(0.0);
        title_label.add_css_class("pane-title");
        let meta_label = gtk4::Label::new(None);
        meta_label.add_css_class("pane-title");
        meta_label.add_css_class("dim");
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let resume_button = gtk4::Button::with_label("Resume");
        resume_button.set_visible(false);
        resume_button.add_css_class("suggested-action");
        header.append(&title_label);
        header.append(&meta_label);
        header.append(&spacer);
        header.append(&resume_button);

        let term = vte4::Terminal::new();
        term.set_scrollback_lines(5000);
        term.set_allow_hyperlink(true);
        term.set_vexpand(true);
        term.set_hexpand(true);

        vbox.append(&header);
        vbox.append(&term);
        frame.set_child(Some(&vbox));

        let w = Rc::new(PaneWidget {
            frame,
            term,
            title_label,
            meta_label,
            resume_button,
            pane_id: pane_id.to_string(),
            actor,
            live: Cell::new(true),
            last_size: Cell::new((80, 24)),
        });

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
        // Resume button for restored tombstones.
        {
            let pid = pane_id.to_string();
            let on_resume = cb.on_resume;
            w.resume_button.connect_clicked(move |_| on_resume(&pid));
        }

        // Attach + initial snapshot.
        match w.actor.attach(pane_id, 80, 24) {
            Ok(snap) => {
                w.term.reset(true, true);
                if !snap.snapshot.is_empty() {
                    w.term.feed(&snap.snapshot);
                }
            }
            Err(e) => {
                w.term
                    .feed(format!("\r\n[attach failed: {e}]\r\n").as_bytes());
            }
        }
        w
    }

    pub fn feed(&self, data: &[u8]) {
        self.term.feed(data);
    }

    pub fn focus(&self) {
        self.term.grab_focus();
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
        self.title_label.set_text(&pane.title);
        let agent = format!("{:?}", pane.agent.kind).to_lowercase();
        self.meta_label
            .set_text(&format!("{} · {}", agent, pane.lifecycle.as_str()));
        for cls in [
            "attention-unread",
            "attention-input",
            "attention-permission",
            "attention-warning",
            "attention-error",
        ] {
            self.frame.remove_css_class(cls);
        }
        let css = attention_css(pane.attention.as_str());
        if !css.is_empty() {
            self.frame.add_css_class(css);
        }
        let live = matches!(pane.live, LiveState::Live);
        let was_live = self.live.replace(live);
        if was_live && !live {
            self.term.feed(b"\r\n[process exited]\r\n");
        }
        // Resume affordance for restored/resumable tombstones.
        let resumable = !live && pane.agent.resume_argv.is_some();
        self.resume_button.set_visible(resumable);
    }
}

//! Declarative detection overlays: herdr-style `agents/*.toml` as data,
//! not code. A manifest extends ONE existing adapter kind (taxonomy stays
//! typed — see FR-002): extra binaries, per-hook lifecycle overrides, a
//! session payload key, a resume template, a display-name override.
//! Parsing is pure (no files here); the server reads the dir and holds
//! the resulting `OverlayAdapter`s. See docs/07, ADR-0009.

use std::collections::HashMap;

use serde::Deserialize;
use signaltty_core::model::AgentKind;
use signaltty_core::state::{Attention, Lifecycle};

use crate::registry::adapter_for_kind;
use crate::screen::{ScreenRule, ScreenRuleSpec};
use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, AnswerChannel, LifecycleDecision,
    NotificationDraft, ProcessInfo, ResumeCommand,
};

/// Parsed `agents/<name>.toml`. Unknown fields ignored (forward compat).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub agent: AgentSection,
    #[serde(default)]
    pub session: SessionSection,
    #[serde(default)]
    pub lifecycle: HashMap<String, HookOverride>,
    #[serde(default)]
    pub screen: Vec<ScreenRuleSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentSection {
    /// Required; must parse as an existing [`AgentKind`].
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionSection {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub resume: Option<Vec<String>>,
}

/// Per-hook `{lifecycle?, attention?, message?}` override. Each field
/// falls through to the builtin decision independently when absent.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookOverride {
    #[serde(default)]
    pub lifecycle: Option<String>,
    #[serde(default)]
    pub attention: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

impl Manifest {
    pub fn kind(&self) -> Result<AgentKind, String> {
        AgentKind::parse(&self.agent.kind)
            .ok_or_else(|| format!("unknown agent kind '{}'", self.agent.kind))
    }
}

/// Parse one manifest file's text. Loud per-file errors; the server
/// isolates failures across files.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
    let kind = manifest.kind()?;
    // Enum strings are validated now so a typo fails at load, not at 2am
    // when the hook first fires.
    if manifest.agent.binaries.iter().any(|b| b.is_empty()) {
        return Err("binaries must not contain empty names".to_string());
    }
    for (hook, ov) in &manifest.lifecycle {
        if let Some(l) = &ov.lifecycle {
            Lifecycle::parse(l).ok_or_else(|| format!("[lifecycle.{hook}] bad lifecycle '{l}'"))?;
        }
        if let Some(a) = &ov.attention {
            Attention::parse(a).ok_or_else(|| format!("[lifecycle.{hook}] bad attention '{a}'"))?;
        }
    }
    for rule in &manifest.screen {
        ScreenRule::compile(rule)?;
    }
    let _ = kind;
    Ok(manifest)
}

/// An adapter overlay: builtin behavior with manifest merges applied.
/// Owned + concrete so the server can hold a `Vec` without globals.
#[derive(Debug, Clone)]
pub struct OverlayAdapter {
    manifest: Manifest,
    screen: Vec<ScreenRule>,
    /// Display override, leaked once here so `metadata()` (called on
    /// every hook) does not leak a fresh copy each time.
    display_name: Option<&'static str>,
}

impl OverlayAdapter {
    pub fn new(manifest: Manifest) -> Result<OverlayAdapter, String> {
        manifest.kind()?; // validate taxonomy now
        let screen = manifest
            .screen
            .iter()
            .map(ScreenRule::compile)
            .collect::<Result<_, _>>()?;
        let display_name = manifest
            .agent
            .display_name
            .as_deref()
            .map(to_static_display);
        Ok(OverlayAdapter {
            manifest,
            screen,
            display_name,
        })
    }

    /// Compiled `[[screen]]` rules, in declaration order.
    pub fn screen_rules(&self) -> &[ScreenRule] {
        &self.screen
    }

    /// The `[[screen]]` rules that apply to a pane running `process`: a
    /// generic manifest with `binaries` only applies to those programs
    /// (spec 033); any other manifest applies to every pane of its kind.
    pub fn screen_rules_for(&self, process: &str) -> &[ScreenRule] {
        let binaries = &self.manifest.agent.binaries;
        let scoped = self.kind() == AgentKind::Generic && !binaries.is_empty();
        if scoped && !binaries.iter().any(|b| b == process) {
            return &[];
        }
        &self.screen
    }

    pub fn kind(&self) -> AgentKind {
        self.manifest.kind().unwrap_or(AgentKind::Generic)
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn builtin(&self) -> &'static dyn AgentAdapter {
        adapter_for_kind(self.kind())
    }
}

impl AgentAdapter for OverlayAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        self.builtin().identify(proc)
            || self
                .manifest
                .agent
                .binaries
                .iter()
                .any(|b| b == proc.bin_name())
    }

    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision {
        let base = self.builtin().lifecycle_state(ev);
        let Some(ov) = self.manifest.lifecycle.get(&ev.hook) else {
            return base;
        };
        LifecycleDecision {
            lifecycle: ov
                .lifecycle
                .as_deref()
                .and_then(Lifecycle::parse)
                .or(base.lifecycle),
            attention: ov
                .attention
                .as_deref()
                .and_then(Attention::parse)
                .or(base.attention),
            message: ov.message.clone().or(base.message),
        }
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        if let Some(key) = self.manifest.session.key.as_deref() {
            if let Some(id) = ev.payload_str(key) {
                return Some(id);
            }
        }
        self.builtin().session_identity(ev)
    }

    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft> {
        self.builtin().notification_event(ev)
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        if let Some(template) = self.manifest.session.resume.as_deref() {
            return Some(ResumeCommand {
                argv: template
                    .iter()
                    .map(|part| part.replace("{session_id}", session_id))
                    .collect(),
            });
        }
        self.builtin().resume_capability(session_id)
    }

    fn answer_channel(&self) -> Option<AnswerChannel> {
        self.builtin().answer_channel()
    }

    fn metadata(&self) -> AdapterMetadata {
        let base = self.builtin().metadata();
        // Merged binaries are informative only (identify does the work);
        // leaking them here would need an owned slice, so keep builtins'.
        AdapterMetadata {
            kind: base.kind,
            display_name: self.display_name.unwrap_or(base.display_name),
            binaries: base.binaries,
        }
    }
}

/// Display overrides are rare and static-friendly: leak one copy per
/// distinct name so `metadata()` keeps its `&'static` shape and repeated
/// `agents.reload`s reuse it instead of leaking again.
fn to_static_display(s: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(name) = names.iter().find(|n| **n == s) {
        return name;
    }
    let name: &'static str = Box::leak(s.to_string().into_boxed_str());
    names.push(name);
    name
}

/// Spawn detection honoring overlays: extra binaries promote before the
/// builtin pass; unknown binaries still fall back to Generic.
pub fn detect_kind_with_overlays(argv: &[String], overlays: &[OverlayAdapter]) -> AgentKind {
    let proc = ProcessInfo {
        argv: argv.to_vec(),
        cwd: String::new(),
        pid: None,
    };
    for overlay in overlays {
        let kind = overlay.kind();
        if matches!(kind, AgentKind::Generic | AgentKind::None) {
            continue;
        }
        if overlay.identify(&proc) {
            return kind;
        }
    }
    crate::registry::detect_kind(argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WRAP: &str = r#"
[agent]
kind = "codex"
display_name = "Codex (wrap)"
binaries = ["codex-wrap"]

[session]
key = "thread_id"
resume = ["codex", "resume", "{session_id}", "--wrap"]

[lifecycle.SomeFutureHook]
lifecycle = "working"
message = "future says hi"
"#;

    fn wrap() -> OverlayAdapter {
        OverlayAdapter::new(parse_manifest(WRAP).unwrap()).unwrap()
    }

    fn proc_of(bin: &str) -> ProcessInfo {
        ProcessInfo {
            argv: vec![bin.to_string()],
            cwd: String::new(),
            pid: None,
        }
    }

    fn ev(hook: &str, payload: serde_json::Value) -> AdapterEvent {
        AdapterEvent {
            agent: "codex".into(),
            hook: hook.into(),
            payload,
        }
    }

    #[test]
    fn valid_manifest_parses_with_unknown_fields_ignored() {
        let m = parse_manifest(&WRAP.replace("binaries =", "extra_future_key = 1\nbinaries ="))
            .unwrap();
        assert_eq!(m.kind(), Ok(AgentKind::Codex));
        assert_eq!(m.agent.binaries, vec!["codex-wrap".to_string()]);
        assert_eq!(m.session.key.as_deref(), Some("thread_id"));
    }

    #[test]
    fn bad_kind_and_bad_enums_fail_loud() {
        assert!(parse_manifest("[agent]\nkind = \"hal9000\"").is_err());
        assert!(parse_manifest(
            "[agent]\nkind = \"codex\"\n[lifecycle.X]\nlifecycle = \"eventually\""
        )
        .is_err());
        assert!(
            parse_manifest("[agent]\nkind = \"codex\"\n[lifecycle.X]\nattention = \"loud\"")
                .is_err()
        );
        assert!(parse_manifest("[agent]\nkind = \"codex\"\nbinaries = [\"\"]").is_err());
        let bad_screen = "[agent]\nkind = \"generic\"\n[[screen]]\nid = \"x\"\nstate = \"working\"\nregex = [\"(\"]";
        assert!(parse_manifest(bad_screen)
            .unwrap_err()
            .contains("[[screen]] 'x'"));
    }

    #[test]
    fn reloading_a_display_name_reuses_its_static_copy() {
        let load = || OverlayAdapter::new(parse_manifest(WRAP).unwrap()).unwrap();
        let first = load().metadata().display_name;
        let again = load().metadata().display_name;
        assert_eq!(first, "Codex (wrap)");
        // Same text, same leaked copy: repeated reloads do not grow memory.
        assert!(std::ptr::eq(first, again));
    }

    #[test]
    fn only_generic_screen_rules_are_scoped_to_binaries() {
        let with = |kind: &str| {
            OverlayAdapter::new(
                parse_manifest(&format!(
                    "[agent]\nkind = \"{kind}\"\nbinaries = [\"wrap\"]\n[[screen]]\nid = \"r\"\nstate = \"idle\"\nregex = ['x']\n"
                ))
                .unwrap(),
            )
            .unwrap()
        };
        // A specific kind's binaries only add detection names.
        assert_eq!(with("codex").screen_rules_for("codex").len(), 1);
        assert_eq!(with("codex").screen_rules_for("wrap").len(), 1);
        // A generic manifest applies only to its own programs.
        assert_eq!(with("generic").screen_rules_for("wrap").len(), 1);
        assert!(with("generic").screen_rules_for("bash").is_empty());
        // Without binaries a generic manifest applies to every generic pane.
        let open = OverlayAdapter::new(
            parse_manifest("[agent]\nkind = \"generic\"\n[[screen]]\nid = \"r\"\nstate = \"idle\"\nregex = ['x']\n").unwrap(),
        )
        .unwrap();
        assert_eq!(open.screen_rules_for("bash").len(), 1);
    }

    #[test]
    fn screen_rules_compile_into_the_overlay() {
        let text = r#"
[agent]
kind = "generic"

[[screen]]
id = "ask"
state = "blocked"
regex = ['Allow\? \[y/n\]']
priority = 3

[[screen]]
id = "spin"
state = "working"
region = "title"
regex = ['^\* ']
"#;
        let overlay = OverlayAdapter::new(parse_manifest(text).unwrap()).unwrap();
        let rules = overlay.screen_rules();
        assert_eq!(rules.len(), 2);
        let hit = crate::screen::classify(rules, "* busy", "Allow? [y/n]").unwrap();
        assert_eq!(
            (hit.id.as_str(), hit.state),
            ("ask", crate::screen::ScreenState::Blocked)
        );
        assert!(wrap().screen_rules().is_empty());
    }

    #[test]
    fn identify_merges_binaries_and_detection_prefers_overlays() {
        let o = wrap();
        assert!(o.identify(&proc_of("codex-wrap")));
        assert!(o.identify(&proc_of("/usr/bin/codex")));
        assert!(!o.identify(&proc_of("claude")));
        assert_eq!(
            detect_kind_with_overlays(&["codex-wrap".to_string()], std::slice::from_ref(&o)),
            AgentKind::Codex
        );
        assert_eq!(
            detect_kind_with_overlays(&["sh".to_string()], std::slice::from_ref(&o)),
            AgentKind::Generic
        );
    }

    #[test]
    fn lifecycle_overrides_field_by_field_with_builtin_fallthrough() {
        let o = wrap();
        // Mapped hook: override wins per field, builtin fills the rest.
        let d = o.lifecycle_state(&ev("SomeFutureHook", json!({})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Working));
        assert_eq!(d.attention, None);
        assert_eq!(d.message.as_deref(), Some("future says hi"));
        // Unmapped hook: pure builtin (Stop → done + unread).
        let d = o.lifecycle_state(&ev("Stop", json!({})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Done));
        assert_eq!(d.attention, Some(Attention::Unread));
    }

    #[test]
    fn session_key_resume_template_and_display_override() {
        let o = wrap();
        assert_eq!(
            o.session_identity(&ev("X", json!({"thread_id": "t-1"}))),
            Some("t-1".to_string())
        );
        // Falls back to the builtin key when the override is absent.
        assert_eq!(
            o.session_identity(&ev("X", json!({"session_id": "s-1"}))),
            Some("s-1".to_string())
        );
        let r = o.resume_capability("abc").unwrap();
        assert_eq!(r.argv, vec!["codex", "resume", "abc", "--wrap"]);
        assert_eq!(o.metadata().display_name, "Codex (wrap)");
        // The override is leaked once per overlay, not once per call.
        assert!(std::ptr::eq(
            o.metadata().display_name,
            o.metadata().display_name
        ));
        assert_eq!(o.metadata().kind, AgentKind::Codex);
        // Channel passes through to the builtin (codex: typed text).
        assert_eq!(o.answer_channel(), Some(AnswerChannel::TypeText));
    }
}

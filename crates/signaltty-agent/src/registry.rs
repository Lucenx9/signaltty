//! Adapter registry: name/kind lookup and argv-based detection.

use signaltty_core::model::AgentKind;

use crate::adapters::{
    claude::ClaudeAdapter, codex::CodexAdapter, cursor::CursorAdapter, generic::GenericAdapter,
    opencode::OpencodeAdapter, pi::PiAdapter,
};
use crate::types::{AgentAdapter, ProcessInfo};

static CODEX: CodexAdapter = CodexAdapter;
static CLAUDE: ClaudeAdapter = ClaudeAdapter;
static OPENCODE: OpencodeAdapter = OpencodeAdapter;
static CURSOR: CursorAdapter = CursorAdapter;
static PI: PiAdapter = PiAdapter;
static GENERIC: GenericAdapter = GenericAdapter;

pub fn all_adapters() -> [&'static dyn AgentAdapter; 6] {
    [&CODEX, &CLAUDE, &OPENCODE, &CURSOR, &PI, &GENERIC]
}

pub fn adapter_for_kind(kind: AgentKind) -> &'static dyn AgentAdapter {
    match kind {
        AgentKind::Codex => &CODEX,
        AgentKind::Claude => &CLAUDE,
        AgentKind::Opencode => &OPENCODE,
        AgentKind::Cursor => &CURSOR,
        AgentKind::Pi => &PI,
        AgentKind::Generic | AgentKind::None => &GENERIC,
    }
}

/// Resolve the `agent` field of a hook-event. Returns None for unknown
/// names (caller reports BAD_PARAMS).
pub fn adapter_for_name(name: &str) -> Option<&'static dyn AgentAdapter> {
    match name {
        "codex" => Some(&CODEX),
        "claude" => Some(&CLAUDE),
        "opencode" => Some(&OPENCODE),
        "cursor" => Some(&CURSOR),
        "pi" => Some(&PI),
        "generic" | "none" => Some(&GENERIC),
        _ => None,
    }
}

/// Detect the agent kind from a spawn command. Specific adapters win;
/// unknown binaries fall back to Generic (plain terminal).
pub fn detect_kind(argv: &[String]) -> AgentKind {
    let proc = ProcessInfo {
        argv: argv.to_vec(),
        cwd: String::new(),
        pid: None,
    };
    for adapter in all_adapters() {
        let kind = adapter.metadata().kind;
        if matches!(kind, AgentKind::Generic | AgentKind::None) {
            continue;
        }
        if adapter.identify(&proc) {
            return kind;
        }
    }
    AgentKind::Generic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_prefers_specific() {
        let argv = |bins: &[&str]| bins.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(detect_kind(&argv(&["codex"])), AgentKind::Codex);
        assert_eq!(
            detect_kind(&argv(&["/usr/local/bin/claude"])),
            AgentKind::Claude
        );
        assert_eq!(detect_kind(&argv(&["opencode"])), AgentKind::Opencode);
        assert_eq!(detect_kind(&argv(&["cursor-agent"])), AgentKind::Cursor);
        assert_eq!(detect_kind(&argv(&["pi"])), AgentKind::Pi);
        assert_eq!(detect_kind(&argv(&["agent"])), AgentKind::Generic);
        assert_eq!(detect_kind(&argv(&["sh"])), AgentKind::Generic);
        assert_eq!(detect_kind(&argv(&[])), AgentKind::Generic);
    }

    #[test]
    fn only_codex_advertises_a_channel() {
        for name in ["codex", "claude", "opencode", "cursor", "pi", "generic"] {
            let channel = adapter_for_name(name).unwrap().answer_channel();
            assert_eq!(channel.is_some(), name == "codex", "{name}");
        }
    }

    #[test]
    fn names_resolve() {
        assert!(adapter_for_name("codex").is_some());
        assert!(adapter_for_name("pi").is_some());
        assert!(adapter_for_name("nope").is_none());
        assert_eq!(
            adapter_for_kind(AgentKind::Claude).metadata().kind,
            AgentKind::Claude
        );
        assert_eq!(
            adapter_for_kind(AgentKind::Pi).metadata().kind,
            AgentKind::Pi
        );
    }
}

//! Screen rules shipped with signaltty (`screen/*.toml`), ported from herdr.
//! Parsed through [`parse_manifest`] so they obey the same validation as
//! user manifests. User `[[screen]]` rules of a kind replace these (docs/07,
//! ADR-0025).

use std::sync::OnceLock;

use signaltty_core::model::AgentKind;

use crate::manifest::parse_manifest;
use crate::screen::ScreenRule;

const BUNDLED: &[&str] = &[
    include_str!("../screen/pi.toml"),
    include_str!("../screen/opencode.toml"),
    include_str!("../screen/cursor.toml"),
];

/// Bundled `[[screen]]` rules for `kind`, in declaration order.
pub fn bundled_screen_rules(kind: AgentKind) -> &'static [ScreenRule] {
    static RULES: OnceLock<Vec<(AgentKind, Vec<ScreenRule>)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        BUNDLED
            .iter()
            .map(|text| {
                let manifest = parse_manifest(text).expect("bundled screen manifest");
                let rules = manifest
                    .screen
                    .iter()
                    .map(|r| ScreenRule::compile(r).expect("bundled screen rule"))
                    .collect();
                (manifest.kind().expect("bundled kind"), rules)
            })
            .collect()
    });
    rules
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, r)| r.as_slice())
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{classify, ScreenState};

    fn state(kind: AgentKind, screen: &str) -> Option<(ScreenState, String)> {
        classify(bundled_screen_rules(kind), "", screen).map(|r| (r.state, r.id.clone()))
    }

    fn assert_state(kind: AgentKind, screen: &str, want: ScreenState, rule: &str) {
        assert_eq!(
            state(kind, screen),
            Some((want, rule.to_string())),
            "{kind:?} on:\n{screen}"
        );
    }

    #[test]
    fn every_bundled_kind_has_rules_ending_in_an_idle_fallback() {
        for kind in [AgentKind::Pi, AgentKind::Opencode, AgentKind::Cursor] {
            let rules = bundled_screen_rules(kind);
            assert!(!rules.is_empty(), "{kind:?}");
            let lowest = rules.iter().min_by_key(|r| r.priority).unwrap();
            assert_eq!(
                (lowest.id.as_str(), lowest.state),
                ("idle_fallback", ScreenState::Idle)
            );
        }
        assert!(bundled_screen_rules(AgentKind::Claude).is_empty());
        assert!(bundled_screen_rules(AgentKind::Generic).is_empty());
    }

    #[test]
    fn pi_screens() {
        use ScreenState::*;
        let pi = AgentKind::Pi;
        assert_state(
            pi,
            "> fix the tests\n\nWorking...\n",
            Working,
            "working_literal",
        );
        assert_state(pi, "> fix\n⠙ Working\n", Working, "working_literal");
        assert_state(
            pi,
            "> fix\n── ⠹ Working ────────\n> ",
            Working,
            "working_border",
        );
        assert_state(pi, "> fix\nDone in 3s.\n> ", Idle, "idle_fallback");
        // A spinner line that is not exactly "<spinner> Working" stays idle.
        assert_state(pi, "⠙ Working on it\n", Idle, "idle_fallback");
    }

    #[test]
    fn opencode_screens() {
        use ScreenState::*;
        let oc = AgentKind::Opencode;
        assert_state(
            oc,
            "△ Permission required\nbash: rm -rf build\n",
            Blocked,
            "permission_required",
        );
        assert_state(
            oc,
            "Pick a file\n↑↓ select  enter confirm  esc dismiss\n",
            Blocked,
            "dialog_required",
        );
        assert_state(
            oc,
            "Questions\n⇆ tab  enter submit  esc dismiss\n",
            Blocked,
            "dialog_required",
        );
        // A dialog footer without navigation hints is not a blocker.
        assert_state(oc, "enter confirm  esc dismiss\n", Idle, "idle_fallback");
        assert_state(
            oc,
            "Thinking\n  esc to interrupt\n",
            Working,
            "interrupt_hint_working",
        );
        assert_state(
            oc,
            "opencode  esc again to interrupt\n",
            Working,
            "interrupt_hint_working",
        );
        assert_state(oc, "Building ■■■■⬝⬝⬝⬝\n", Working, "progress_bar_working");
        assert_state(oc, "■■■ short bar\n", Idle, "idle_fallback");
        // Blocked outranks working.
        assert_state(
            oc,
            "△ Permission required\nesc to interrupt\n",
            Blocked,
            "permission_required",
        );
    }

    #[test]
    fn cursor_screens() {
        use ScreenState::*;
        let cur = AgentKind::Cursor;
        assert_state(
            cur,
            "Write to this file?\nsrc/main.rs\nProceed (y)\nReject & propose changes (p)\n",
            Blocked,
            "write_file_approval",
        );
        assert_state(
            cur,
            "Waiting for approval\nRun this command?\n  Run (once) (y)\n",
            Blocked,
            "approval_run_once",
        );
        assert_state(
            cur,
            "  Allow read(/etc/hosts) (y)\n",
            Blocked,
            "approval_prompt",
        );
        assert_state(cur, "→ Run cargo test (y)\n", Blocked, "approval_prompt");
        assert_state(cur, "Keep (n)\n", Blocked, "approval_prompt");
        assert_state(
            cur,
            "Reading files\nctrl+c to stop\n",
            Working,
            "stop_hint_working",
        );
        assert_state(
            cur,
            "2 background tasks\n",
            Working,
            "background_task_status_working",
        );
        assert_state(cur, "  ⬢ Generating\n", Working, "spinner_working");
        assert_state(cur, "⠋ Thinking hard\n", Working, "spinner_working");
        // The stop hint only counts in the last six non-empty lines.
        let old_hint = "ctrl+c to stop\na\nb\nc\nd\ne\nf\n";
        assert_state(cur, old_hint, Idle, "idle_fallback");
        assert_state(cur, "0 background tasks\n> ", Idle, "idle_fallback");
        // herdr matches this per line: a count ending one line never pairs
        // with "background tasks" on the next.
        assert_state(
            cur,
            "error count: 1\nbackground tasks panel\n",
            Idle,
            "idle_fallback",
        );
    }
}

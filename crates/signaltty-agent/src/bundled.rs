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
    include_str!("../screen/claude.toml"),
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
        for kind in [
            AgentKind::Pi,
            AgentKind::Opencode,
            AgentKind::Cursor,
            AgentKind::Claude,
        ] {
            let rules = bundled_screen_rules(kind);
            assert!(!rules.is_empty(), "{kind:?}");
            let lowest = rules.iter().min_by_key(|r| r.priority).unwrap();
            assert_eq!(
                (lowest.id.as_str(), lowest.state),
                ("idle_fallback", ScreenState::Idle)
            );
        }
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

    const RULE: &str = "────────────────────────────────";

    /// A Claude screen: `above` the prompt box, the box body, then the footer.
    fn claude(above: &str, body: &str, footer: &str) -> String {
        format!("{above}\n{RULE}\n{body}\n{RULE}\n{footer}\n")
    }

    fn claude_title(title: &str, screen: &str) -> Option<(ScreenState, String)> {
        classify(bundled_screen_rules(AgentKind::Claude), title, screen)
            .map(|r| (r.state, r.id.clone()))
    }

    #[test]
    fn claude_working_screens() {
        use ScreenState::*;
        let c = AgentKind::Claude;
        let idle_box = |above: &str| claude(above, "❯ ", "  ? for shortcuts");
        assert_state(
            c,
            &idle_box("✻ Cogitating… (12s · ↓ 1.2k tokens · esc to interrupt)"),
            Working,
            "live_turn_working",
        );
        assert_state(
            c,
            &claude(
                "> fix",
                "❯ ",
                "  ⏵⏵ accept edits on (shift+tab to cycle) · esc to interrupt",
            ),
            Working,
            "live_turn_working",
        );
        // Background agents above the box outrank the idle prompt box.
        assert_state(
            c,
            &idle_box("> fix\n✻ Waiting for 2 background agents to finish"),
            Working,
            "background_agents_working",
        );
        assert_state(
            c,
            &idle_box("✢ Syncing · 1 MCP task still running"),
            Working,
            "background_mcp_task_working",
        );
        // ...but not while a permission prompt is on screen.
        assert_state(
            c,
            &idle_box("✢ Syncing · 1 MCP task still running\nDo you want to proceed?"),
            Idle,
            "live_prompt_box",
        );
        assert_state(
            c,
            &claude("> hi", "  /btw what changed?", "  Esc to close"),
            Working,
            "btw_overlay_working",
        );
        assert_eq!(
            claude_title("⠙ Fixing tests", &idle_box("> hi")),
            Some((Working, "osc_title_working".into()))
        );
    }

    #[test]
    fn claude_blocked_screens() {
        use ScreenState::*;
        let c = AgentKind::Claude;
        assert_state(
            c,
            &format!(
                "{RULE}\n Bash command\n   rm -rf build\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, and don't ask again for: rm\n   3. No\n Esc to cancel · Tab to amend · ctrl+e to explain\n"
            ),
            Blocked,
            "bash_permission_prompt",
        );
        assert_state(
            c,
            &format!(
                "{RULE}\n Edit file src/a.rs\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n Esc to cancel\n"
            ),
            Blocked,
            "generic_permission_prompt",
        );
        assert_state(
            c,
            &format!("{RULE}\n Pick a branch\n ❯ main\n   dev\n Enter to select · ↑/↓ to navigate · Esc to cancel\n"),
            Blocked,
            "live_blocked_form_select",
        );
        assert_state(
            c,
            &format!("{RULE}\n Commit message\n Enter to confirm · Esc to cancel\n"),
            Blocked,
            "live_blocked_form",
        );
        // A form footer or permission prompt that scrolled above the last
        // rule no longer counts for the after-last-rule rules.
        assert_state(
            c,
            &format!(
                " Commit message\n Enter to confirm · Esc to cancel\n{RULE}\n  ? for shortcuts\n"
            ),
            Idle,
            "idle_fallback",
        );
        assert_state(
            c,
            &format!(" Do you want to proceed?\n ❯ 1. Yes\n   2. No\n Esc to cancel\n{RULE}\n  ? for shortcuts\n"),
            Blocked,
            "legacy_no_prompt_blocker",
        );
        assert_state(
            c,
            "Run a dynamic workflow?\n ❯ Yes\n Esc to cancel\n",
            Blocked,
            "dynamic_workflow_prompt",
        );
        assert_state(
            c,
            "MCP server \"github\" requests your input\n ❯ Accept\n   Decline\n Esc to cancel\n",
            Blocked,
            "mcp_elicitation_prompt",
        );
        assert_state(
            c,
            "Would you like to continue? Yes / No\n",
            Blocked,
            "legacy_no_prompt_blocker",
        );
        // A bare prompt marker line vetoes the legacy blocker.
        assert_state(
            c,
            "Would you like to continue? Yes\n❯ \n",
            Idle,
            "idle_fallback",
        );
    }

    #[test]
    fn claude_hold_and_idle_screens() {
        use ScreenState::*;
        let c = AgentKind::Claude;
        // The transcript viewer wins over a working line and changes nothing.
        assert_state(
            c,
            "✻ Cogitating… (3s · esc to interrupt)\nShowing detailed transcript · ctrl+o to toggle\n",
            Hold,
            "transcript_viewer",
        );
        assert_state(
            c,
            "Select model\n ❯ 1. Opus\n   2. Sonnet\n Enter to set as default · Esc to cancel\n",
            Hold,
            "model_picker_menu",
        );
        assert_state(
            c,
            &claude("> done", "❯ ", "  ? for shortcuts"),
            Idle,
            "live_prompt_box",
        );
        // A form inside the box is not an idle prompt.
        assert_state(
            c,
            &claude("> pick", "❯ a\n  Enter to select", "  Esc to cancel"),
            Idle,
            "idle_fallback",
        );
        assert_eq!(
            claude_title("✳ Claude Code", "plain output\n"),
            Some((Idle, "osc_title_idle".into()))
        );
        assert_eq!(
            claude_title("claude", "plain output\n"),
            Some((Idle, "idle_fallback".into()))
        );
    }
}

//! Screen rules shipped with signaltty (`screen/*.toml`), ported from herdr.
//! Parsed through [`parse_manifest`] so they obey the same validation as
//! user manifests. User `[[screen]]` rules of a kind replace these (docs/07,
//! ADR-0025); generic ones are scoped to their `binaries` (ADR-0028).

use std::sync::OnceLock;

use signaltty_core::model::AgentKind;

use crate::manifest::{parse_manifest, OverlayAdapter};
use crate::screen::ScreenRule;

const BUNDLED: &[&str] = &[
    include_str!("../screen/pi.toml"),
    include_str!("../screen/opencode.toml"),
    include_str!("../screen/cursor.toml"),
    include_str!("../screen/claude.toml"),
    include_str!("../screen/codex.toml"),
    include_str!("../screen/gemini.toml"),
    include_str!("../screen/copilot.toml"),
    include_str!("../screen/droid.toml"),
    include_str!("../screen/kilo.toml"),
    include_str!("../screen/qodercli.toml"),
];

/// Bundled `[[screen]]` rules for a pane of `kind` running `process`, in
/// file then declaration order. Generic manifests are scoped to their
/// `binaries` (spec 033); other kinds ignore `process`.
pub fn bundled_screen_rules(kind: AgentKind, process: &str) -> Vec<&'static ScreenRule> {
    static MANIFESTS: OnceLock<Vec<OverlayAdapter>> = OnceLock::new();
    MANIFESTS
        .get_or_init(|| {
            BUNDLED
                .iter()
                .map(|text| {
                    let manifest = parse_manifest(text).expect("bundled screen manifest");
                    OverlayAdapter::new(manifest).expect("bundled screen manifest")
                })
                .collect()
        })
        .iter()
        .filter(|m| m.kind() == kind)
        .flat_map(|m| m.screen_rules_for(process))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{classify, ScreenState};

    fn state(kind: AgentKind, screen: &str) -> Option<(ScreenState, String)> {
        classify(bundled_screen_rules(kind, ""), "", screen).map(|r| (r.state, r.id.clone()))
    }

    /// A generic pane running `process`.
    fn program(process: &str, screen: &str) -> Option<(ScreenState, String)> {
        classify(
            bundled_screen_rules(AgentKind::Generic, process),
            "",
            screen,
        )
        .map(|r| (r.state, r.id.clone()))
    }

    fn assert_program(process: &str, screen: &str, want: ScreenState, rule: &str) {
        assert_eq!(
            program(process, screen),
            Some((want, rule.to_string())),
            "{process} on:\n{screen}"
        );
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
            AgentKind::Codex,
        ] {
            let rules = bundled_screen_rules(kind, "");
            assert!(!rules.is_empty(), "{kind:?}");
            let lowest = rules.iter().min_by_key(|r| r.priority).unwrap();
            assert_eq!(
                (lowest.id.as_str(), lowest.state),
                ("idle_fallback", ScreenState::Idle)
            );
        }
        // Generic rules only apply to their own programs, shells excluded.
        for process in [
            "gemini", "copilot", "ghcs", "droid", "kilo", "qodercli", "qoder",
        ] {
            let rules = bundled_screen_rules(AgentKind::Generic, process);
            assert!(!rules.is_empty(), "{process}");
            let lowest = rules.iter().min_by_key(|r| r.priority).unwrap();
            assert_eq!(lowest.id.as_str(), "idle_fallback", "{process}");
        }
        for process in ["bash", "sh", "", "geminix", "codex"] {
            assert!(
                bundled_screen_rules(AgentKind::Generic, process).is_empty(),
                "{process}"
            );
        }
        // A program's rules never leak into another program's.
        let gemini = bundled_screen_rules(AgentKind::Generic, "gemini");
        assert!(gemini.iter().all(|r| r.id != "esc_interrupt_working"));
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
        classify(bundled_screen_rules(AgentKind::Claude, ""), title, screen)
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

    fn codex(title: &str, screen: &str) -> Option<(ScreenState, String)> {
        classify(bundled_screen_rules(AgentKind::Codex, ""), title, screen)
            .map(|r| (r.state, r.id.clone()))
    }

    #[test]
    fn codex_title_screens() {
        use ScreenState::*;
        let idle = "› \n";
        assert_eq!(
            codex("⚠ Action Required", idle),
            Some((Blocked, "osc_title_blocked".into()))
        );
        assert_eq!(
            codex("⠙ codex", idle),
            Some((Working, "osc_title_working".into()))
        );
        assert_eq!(codex("codex", idle), Some((Idle, "osc_title_idle".into())));
        assert_eq!(codex("", idle), Some((Idle, "idle_fallback".into())));
        // The idle title rule itself vetoes spinners and Action Required, even
        // though higher-priority title rules usually win first.
        let rule = bundled_screen_rules(AgentKind::Codex, "")
            .into_iter()
            .find(|r| r.id == "osc_title_idle")
            .unwrap();
        assert!(rule.matches("codex", idle));
        assert!(!rule.matches("⠙ codex", idle));
        assert!(!rule.matches("Action Required", idle));
    }

    #[test]
    fn codex_blocked_screens() {
        use ScreenState::*;
        let x = AgentKind::Codex;
        assert_state(
            x,
            "> You are in /home/me/repo\n\nDo you trust the contents of this directory?\n› 1. Yes, continue\n",
            Blocked,
            "trust_directory",
        );
        // The trust prompt is read from the top, however long the screen.
        let long = format!(
            "> You are in /repo\nDo you trust the contents of this directory?\n{}",
            (1..=25)
                .map(|i| format!("  option {i}\n"))
                .collect::<String>()
        );
        assert_state(x, &long, Blocked, "trust_directory");
        assert_state(
            x,
            "Folder access\nTrust this folder?\nCodex can read, edit, and run files here\n  Trust and continue\n",
            Blocked,
            "trust_folder",
        );
        assert_state(
            x,
            "✨ Update available! 0.9 -> 1.0\n› 1. Update now\n  2. Skip\n  3. Skip until next version\n\nPress enter to continue\n",
            Blocked,
            "startup_update",
        );
        // The update screen only blocks while it ends in the continue hint.
        assert_state(
            x,
            "Update available!\nUpdate now\nSkip until next version\nPress enter to continue\nupdating...\n",
            Idle,
            "idle_fallback",
        );
        assert_state(
            x,
            "› run tests\n  Allow command?\n  cargo test\n  Press enter to confirm or esc to cancel\n",
            Blocked,
            "live_strong_blocker",
        );
        assert_state(
            x,
            "› /mention\n  All Results  Filesystem Only  Plugins\n",
            Blocked,
            "live_strong_picker",
        );
        assert_state(
            x,
            "• Ran rm -rf build\nProceed? [y/n]\n",
            Blocked,
            "weak_blocker",
        );
        // A current prompt empties the weak-blocker region.
        assert_state(x, "Proceed? [y/n]\n› \n", Idle, "idle_fallback");
        // So does a sparkle prompt with no response after it.
        assert_state(
            x,
            "• Do you want to retry? yes\n›⠂ typing\n",
            Idle,
            "idle_fallback",
        );
    }

    #[test]
    fn codex_working_and_hold_screens() {
        use ScreenState::*;
        let x = AgentKind::Codex;
        assert_state(
            x,
            "› fix the tests\n• Working (12s • esc to interrupt)\n› \n",
            Working,
            "screen_working_fallback",
        );
        assert_state(
            x,
            "› fix\n• Thinking (1m 3s)\n• Queued follow-up inputs\n› \n",
            Working,
            "screen_working_fallback",
        );
        // A response after the timer means the turn is over.
        assert_state(x, "• Working (12s)\n✓ Done\n› \n", Idle, "idle_fallback");
        // A failed reconnect keeps its timer but is not working.
        assert_state(
            x,
            "• Reconnect failed — check the endpoint, then relaunch (5s)\n› \n",
            Idle,
            "idle_fallback",
        );
        assert_state(
            x,
            "› q\n  ↑/↓ to scroll  pgup/pgdn to page  home/end to jump  q to quit  esc to edit prev\n",
            Hold,
            "transcript_viewer",
        );
    }

    #[test]
    fn gemini_screens() {
        use ScreenState::*;
        assert_program(
            "gemini",
            "│ Apply this change?\n│ ● 1. Yes\n",
            Blocked,
            "apply_or_allow_change",
        );
        assert_program(
            "gemini",
            "│ Allow execution of: rm -rf build\n",
            Blocked,
            "apply_or_allow_change",
        );
        assert_program(
            "gemini",
            "  ❯ 1. Yes, allow once\n",
            Blocked,
            "apply_or_allow_change",
        );
        assert_program(
            "gemini",
            "Waiting for user confirmation...\n 1. Yes\n",
            Blocked,
            "confirmation_prompt",
        );
        assert_program(
            "gemini",
            "⠋ Thinking (esc to cancel, 3s)\n",
            Working,
            "esc_cancel_working",
        );
        // Blocked outranks the cancel hint on the same screen.
        assert_program(
            "gemini",
            "│ Apply this change?\n(esc to cancel)\n",
            Blocked,
            "apply_or_allow_change",
        );
        assert_program("gemini", "> \n", Idle, "idle_fallback");
        // "yes" alone is not a confirmation.
        assert_program("gemini", "yes, that worked\n", Idle, "idle_fallback");
        // herdr needs the box rule or the question mark around "proceed".
        assert_program(
            "gemini",
            "Do you want to proceed to step 2: yes\n",
            Idle,
            "idle_fallback",
        );
        assert_program(
            "gemini",
            "│ Do you want to proceed\n 1. Yes\n",
            Blocked,
            "confirmation_prompt",
        );
    }

    #[test]
    fn copilot_screens() {
        use ScreenState::*;
        assert_program(
            "copilot",
            "Run this command?\n Enter to confirm · Esc to cancel\n",
            Blocked,
            "selection_blocker",
        );
        assert_program(
            "ghcs",
            "Pick one\nenter accept  esc cancel\n",
            Blocked,
            "selection_blocker",
        );
        assert_program(
            "copilot",
            "> fix\n◎ Waiting for background agents · 2 running\n",
            Working,
            "background_agents_working",
        );
        assert_program(
            "copilot",
            "Thinking… (Esc to cancel)\n",
            Working,
            "working_cancel_hint",
        );
        assert_program(
            "copilot",
            "Thinking… esc interrupt\n",
            Working,
            "working_cancel_hint",
        );
        // The background-agents line only counts in the last six lines.
        let old = "◎ Waiting for background agents\na\nb\nc\nd\ne\nf\n";
        assert_program("copilot", old, Idle, "idle_fallback");
    }

    #[test]
    fn droid_screens() {
        use ScreenState::*;
        assert_program(
            "droid",
            "Execute rm -rf build?\n> Yes, allow\n  No, cancel\nUse ↑↓ to navigate · Enter to select · Esc to cancel\n",
            Blocked,
            "execute_selection_blocker",
        );
        assert_program(
            "droid",
            "Choose model\n ↑↓ navigate  enter select  esc cancel\n",
            Blocked,
            "selection_menu_blocker",
        );
        assert_program(
            "droid",
            "⠙ Running tests (esc to stop)\n",
            Working,
            "spinner_stop_working",
        );
        assert_program(
            "droid",
            "Running tests (esc to stop)\n",
            Working,
            "stop_hint_working",
        );
        // Navigation footer without an allow/cancel choice is the menu rule's job.
        assert_program(
            "droid",
            "↑↓ to navigate · enter to select · esc to cancel\n",
            Idle,
            "idle_fallback",
        );
    }

    #[test]
    fn kilo_and_qodercli_screens() {
        use ScreenState::*;
        assert_program(
            "kilo",
            "△ Permission required\nbash: ls\n",
            Blocked,
            "opencode_permission",
        );
        assert_program(
            "kilo-code",
            "Pick\n↑↓ select  enter confirm  esc dismiss\n",
            Blocked,
            "opencode_permission_dialog",
        );
        assert_program(
            "kilo",
            "Building  esc interrupt\n",
            Working,
            "esc_interrupt_working",
        );
        assert_program(
            "kilo",
            "enter confirm  esc dismiss\n",
            Idle,
            "idle_fallback",
        );

        assert_program(
            "qodercli",
            "Waiting for user confirmation\n  Allow  Reject\n",
            Blocked,
            "confirmation_or_input_blocker",
        );
        assert_program(
            "qoder",
            "Permission required for Bash\n",
            Blocked,
            "confirmation_or_input_blocker",
        );
        assert_program(
            "qodercli",
            "Allow once or always?\n",
            Blocked,
            "confirmation_or_input_blocker",
        );
        assert_program(
            "qodercli",
            "Thinking (esc to cancel, 4s)\n",
            Working,
            "cancel_hint_working",
        );
        assert_program("qodercli", "⠙ Reading files\n", Working, "spinner_working");
        assert_program("qodercli", "⠙\n", Idle, "idle_fallback");
    }
}

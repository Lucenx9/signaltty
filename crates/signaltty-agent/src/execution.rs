//! Native automatic execution for explicit managed launch choices.

/// Add automatic mode only to a supported bare interactive launch command.
pub fn automatic_launch_argv(argv: &[String]) -> Vec<String> {
    let mut launch = argv.to_vec();
    if argv.len() == 1 {
        launch.extend(automatic_flags(program(argv)).iter().map(|s| (*s).into()));
    }
    launch
}

fn program(argv: &[String]) -> &str {
    argv.first()
        .and_then(|s| std::path::Path::new(s).file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
}

fn automatic_flags(program: &str) -> &'static [&'static str] {
    match program {
        "codex" => &["--dangerously-bypass-approvals-and-sandbox"],
        "claude" => &["--dangerously-skip-permissions"],
        "agy" | "antigravity" | "antigravity-cli" => {
            &["--dangerously-skip-permissions", "--mode", "accept-edits"]
        }
        _ => &[],
    }
}

/// Carry our explicit automatic launch prefix into a default same-provider
/// worker or minimal native resume. Commands with explicit options win.
pub fn inherit_automatic_permissions(source: &[String], target: &[String]) -> Vec<String> {
    let name = program(source);
    let flags = automatic_flags(name);
    let standard_resume = target.len() == 3
        && matches!(
            (name, target[1].as_str()),
            ("codex", "resume")
                | ("claude", "--resume")
                | ("agy" | "antigravity" | "antigravity-cli", "--conversation")
        )
        && !target[2].is_empty()
        && !target[2].starts_with('-');
    let automatic = !flags.is_empty()
        && source
            .get(1..1 + flags.len())
            .is_some_and(|args| args.iter().map(String::as_str).eq(flags.iter().copied()));
    if automatic && program(target) == name && (target.len() == 1 || standard_resume) {
        let mut inherited = target.to_vec();
        inherited.splice(1..1, flags.iter().map(|s| (*s).into()));
        inherited
    } else {
        target.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn antigravity_aliases_keep_automatic_launch_and_resume_permissions() {
        for program in ["agy", "antigravity", "antigravity-cli"] {
            let expected = argv(&[
                program,
                "--dangerously-skip-permissions",
                "--mode",
                "accept-edits",
            ]);
            assert_eq!(automatic_launch_argv(&argv(&[program])), expected);
            let resume = argv(&[program, "--conversation", "session"]);
            let mut expected_resume = expected.clone();
            expected_resume.extend(["--conversation".into(), "session".into()]);
            assert_eq!(
                inherit_automatic_permissions(&expected, &resume),
                expected_resume
            );
        }
    }

    #[test]
    fn supported_bare_launches_use_native_automatic_execution() {
        for (input, expected) in [
            (
                &["codex"][..],
                &["codex", "--dangerously-bypass-approvals-and-sandbox"][..],
            ),
            (
                &["/opt/claude"][..],
                &["/opt/claude", "--dangerously-skip-permissions"][..],
            ),
            (
                &["agy"][..],
                &[
                    "agy",
                    "--dangerously-skip-permissions",
                    "--mode",
                    "accept-edits",
                ][..],
            ),
        ] {
            assert_eq!(automatic_launch_argv(&argv(input)), argv(expected));
        }
        for input in [
            &[][..],
            &["bash"][..],
            &["codex-wrapper"][..],
            &["opencode"][..],
            &["codex", "--sandbox", "read-only"][..],
            &["agy", "--mode", "plan"][..],
        ] {
            assert_eq!(automatic_launch_argv(&argv(input)), argv(input));
        }
    }

    #[test]
    fn automatic_inheritance_respects_native_resume_and_explicit_commands() {
        for (source, target, expected) in [
            (
                &["claude", "--dangerously-skip-permissions"][..],
                &["claude", "--resume", "s1"][..],
                &["claude", "--dangerously-skip-permissions", "--resume", "s1"][..],
            ),
            (
                &[
                    "agy",
                    "--dangerously-skip-permissions",
                    "--mode",
                    "accept-edits",
                ][..],
                &["agy", "--conversation", "s2"][..],
                &[
                    "agy",
                    "--dangerously-skip-permissions",
                    "--mode",
                    "accept-edits",
                    "--conversation",
                    "s2",
                ][..],
            ),
        ] {
            let inherited = inherit_automatic_permissions(&argv(source), &argv(target));
            assert_eq!(inherited, argv(expected));
            assert_eq!(
                inherit_automatic_permissions(&argv(source), &inherited),
                inherited
            );
        }
        let source = argv(&["codex", "--dangerously-bypass-approvals-and-sandbox"]);
        assert_eq!(
            inherit_automatic_permissions(&source, &argv(&["codex"])),
            source
        );
        for target in [
            &[][..],
            &["claude"][..],
            &["bash"][..],
            &["codex", "resume", "s", "--sandbox", "read-only"][..],
            &["codex", "resume", "--last"][..],
            &["codex", "--model", "m"][..],
        ] {
            assert_eq!(
                inherit_automatic_permissions(&source, &argv(target)),
                argv(target)
            );
        }
        for source in [
            &[][..],
            &["bash", "-c", "codex"][..],
            &[
                "codex",
                "--model",
                "--dangerously-bypass-approvals-and-sandbox",
            ][..],
            &[
                "codex-wrapper",
                "--dangerously-bypass-approvals-and-sandbox",
            ][..],
            &["codex", "--", "--dangerously-bypass-approvals-and-sandbox"][..],
            &["agy", "--dangerously-skip-permissions", "--mode", "plan"][..],
        ] {
            let target = argv(&["codex", "resume", "s"]);
            assert_eq!(
                inherit_automatic_permissions(&argv(source), &target),
                target
            );
        }
    }
}

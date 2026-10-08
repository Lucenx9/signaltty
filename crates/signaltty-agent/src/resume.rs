//! Resume argv resolution: retains the selected absolute executable
//! when the pane records an absolute launch binary matching the official
//! command's basename.

/// Resolve effective resume argv from the pane's original launch argv and the adapter's
/// official resume argv.
///
/// Explicit adapter paths take precedence. Only matching bare commands are
/// replaced; legacy relative launch paths have no reliable directory anchor.
/// Launch options and the adapter's resume arguments stay separate.
pub fn resolve_resume_argv(original_argv: &[String], resume_argv: &[String]) -> Vec<String> {
    let mut resolved = resume_argv.to_vec();
    if let (Some(selected), Some(program)) = (original_argv.first(), resolved.first_mut()) {
        if std::path::Path::new(selected).is_absolute()
            && !program.is_empty()
            && !program.contains('/')
            && selected.rsplit('/').next() == Some(program.as_str())
        {
            *program = selected.clone();
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_vec(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn builtin_adapters_matrix() {
        let cases = [
            (
                &["/opt/tools/codex"][..],
                &["codex", "resume", "sess-codex"][..],
                &["/opt/tools/codex", "resume", "sess-codex"][..],
            ),
            (
                &["/usr/local/bin/claude"][..],
                &["claude", "--resume", "sess-claude"][..],
                &["/usr/local/bin/claude", "--resume", "sess-claude"][..],
            ),
            (
                &["/opt/bin/opencode"][..],
                &["opencode", "--session", "sess-opencode"][..],
                &["/opt/bin/opencode", "--session", "sess-opencode"][..],
            ),
            (
                &["/home/user/bin/cursor-agent"][..],
                &["cursor-agent", "--resume", "sess-cursor"][..],
                &["/home/user/bin/cursor-agent", "--resume", "sess-cursor"][..],
            ),
            (
                &["/opt/pi/bin/pi"][..],
                &["pi", "--session", "sess-pi"][..],
                &["/opt/pi/bin/pi", "--session", "sess-pi"][..],
            ),
        ];

        for (orig, resume, expected) in cases {
            assert_eq!(
                resolve_resume_argv(&to_vec(orig), &to_vec(resume)),
                to_vec(expected),
                "Failed for adapter case: orig={orig:?}, resume={resume:?}"
            );
        }
    }

    #[test]
    fn legacy_relative_paths_keep_adapter_command() {
        let cases = [
            (
                &["./bin/codex"][..],
                &["codex", "resume", "s1"][..],
                &["codex", "resume", "s1"][..],
            ),
            (
                &["../tools/claude"][..],
                &["claude", "--resume", "s2"][..],
                &["claude", "--resume", "s2"][..],
            ),
            (
                &["vendor/bin/opencode"][..],
                &["opencode", "--session", "s3"][..],
                &["opencode", "--session", "s3"][..],
            ),
        ];

        for (orig, resume, expected) in cases {
            assert_eq!(
                resolve_resume_argv(&to_vec(orig), &to_vec(resume)),
                to_vec(expected),
                "Failed for relative path case: orig={orig:?}, resume={resume:?}"
            );
        }
    }

    #[test]
    fn empty_inputs() {
        assert_eq!(
            resolve_resume_argv(&[], &to_vec(&["codex", "resume", "s"])),
            to_vec(&["codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(&to_vec(&[""]), &to_vec(&["codex", "resume", "s"])),
            to_vec(&["codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(&to_vec(&["/opt/codex"]), &[]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn bare_original_argv() {
        assert_eq!(
            resolve_resume_argv(&to_vec(&["codex"]), &to_vec(&["codex", "resume", "s"])),
            to_vec(&["codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(&to_vec(&["claude"]), &to_vec(&["claude", "--resume", "s"])),
            to_vec(&["claude", "--resume", "s"])
        );
    }

    #[test]
    fn promoted_shells() {
        assert_eq!(
            resolve_resume_argv(&to_vec(&["/bin/bash"]), &to_vec(&["codex", "resume", "s"])),
            to_vec(&["codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/usr/bin/zsh", "-l"]),
                &to_vec(&["claude", "--resume", "s"])
            ),
            to_vec(&["claude", "--resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/bin/sh"]),
                &to_vec(&["opencode", "--session", "s"])
            ),
            to_vec(&["opencode", "--session", "s"])
        );
    }

    #[test]
    fn manifest_explicit_paths_preserved() {
        // Manifest explicit path must NOT be overridden by the helper
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/opt/special/codex"]),
                &to_vec(&["/custom/bin/codex", "resume", "s"])
            ),
            to_vec(&["/custom/bin/codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/opt/claude"]),
                &to_vec(&["./manifest/bin/claude", "--resume", "s"])
            ),
            to_vec(&["./manifest/bin/claude", "--resume", "s"])
        );
    }

    #[test]
    fn suffix_flags_not_copied() {
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&[
                    "/opt/tools/codex",
                    "-m",
                    "gpt-4",
                    "--profile",
                    "dev",
                    "--sandbox",
                    "workspace"
                ]),
                &to_vec(&["codex", "resume", "session-xyz"])
            ),
            to_vec(&["/opt/tools/codex", "resume", "session-xyz"])
        );
    }

    #[test]
    fn wrapper_differing_basename() {
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/usr/local/bin/codex-wrapper"]),
                &to_vec(&["codex", "resume", "s"])
            ),
            to_vec(&["codex", "resume", "s"])
        );
        assert_eq!(
            resolve_resume_argv(
                &to_vec(&["/opt/claude-runner"]),
                &to_vec(&["claude", "--resume", "s"])
            ),
            to_vec(&["claude", "--resume", "s"])
        );
    }
}

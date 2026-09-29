//! Native interactive Codex must host hooks in this pane, not a shared daemon.
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
enum Invocation {
    Local { isolated: bool },
    Remote,
    Unsupported,
    Unchanged,
}

const VALUE_OPTIONS: &[&str] = &[
    "-c",
    "--config",
    "--enable",
    "--disable",
    "-i",
    "--image",
    "-m",
    "--model",
    "--local-provider",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-C",
    "--cd",
    "--add-dir",
    "-a",
    "--ask-for-approval",
    "--remote-auth-token-env",
];

fn invocation(argv: &[String]) -> Invocation {
    if argv
        .first()
        .and_then(|s| Path::new(s).file_name())
        .and_then(|s| s.to_str())
        != Some("codex")
    {
        return Invocation::Unchanged;
    }
    let mut args = argv.iter().skip(1).peekable();
    let mut isolated = false;
    let mut positional = false;
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let inline_image = arg.starts_with("--image=") || (arg.starts_with("-i") && arg.len() > 2);
        if inline_image || matches!(arg.as_str(), "--image" | "-i") {
            let mut has_image = inline_image;
            while args.peek().is_some_and(|value| !value.starts_with('-')) {
                args.next();
                has_image = true;
            }
            if !has_image {
                return Invocation::Unsupported;
            }
            continue;
        }
        match arg.as_str() {
            "--remote" => return Invocation::Remote,
            "--no-daemon" => isolated = true,
            "-h" | "--help" | "-V" | "--version" => return Invocation::Unchanged,
            a if VALUE_OPTIONS.contains(&a) => {
                if args.next().is_none() {
                    return Invocation::Unsupported;
                }
            }
            "--oss"
            | "--approve-for-me"
            | "--dangerously-bypass-approvals-and-sandbox"
            | "--dangerously-bypass-hook-trust"
            | "--worktree"
            | "--search"
            | "--no-alt-screen"
            | "--strict-config"
            | "--last"
            | "--all"
            | "--include-non-interactive" => {}
            a if a.starts_with("--remote=") => return Invocation::Remote,
            a if a.starts_with('-') => {
                let long_value = a
                    .split_once('=')
                    .is_some_and(|(name, _)| VALUE_OPTIONS.contains(&name));
                let short_value = a.len() > 2
                    && VALUE_OPTIONS
                        .iter()
                        .any(|name| name.len() == 2 && a.starts_with(name));
                if !long_value && !short_value {
                    return Invocation::Unsupported;
                }
            }
            a if !positional => {
                positional = true;
                if matches!(
                    a,
                    "agents"
                        | "exec"
                        | "e"
                        | "review"
                        | "login"
                        | "logout"
                        | "mcp"
                        | "plugin"
                        | "app-server"
                        | "remote-control"
                        | "completion"
                        | "update"
                        | "doctor"
                        | "sandbox"
                        | "debug"
                        | "apply"
                        | "a"
                        | "queue"
                        | "archive"
                        | "delete"
                        | "migrate-rollouts"
                        | "unarchive"
                        | "cloud"
                        | "exec-server"
                        | "features"
                        | "help"
                ) {
                    return Invocation::Unchanged;
                }
            }
            _ => {}
        }
    }
    Invocation::Local { isolated }
}

fn supports_no_daemon(program: &str, cwd: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(["--no-daemon", "--version"])
        .current_dir(cwd)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
                return status.success();
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
                let _ = child.wait();
                return false;
            }
        }
    }
}

pub(crate) fn prepare(argv: &[String], cwd: &str) -> (Vec<String>, Option<&'static str>) {
    let mut effective = argv.to_vec();
    let notice = match invocation(argv) {
        Invocation::Local { isolated: true } | Invocation::Unchanged => None,
        Invocation::Unsupported => Some("Codex arguments are not recognized for pane-local runtime setup; status integration is unavailable. The original command remains available."),
        Invocation::Remote => {
            Some("Codex uses a remote app-server; terminal hook attribution is unavailable.")
        }
        Invocation::Local { isolated: false } => {
            if supports_no_daemon(&argv[0], cwd) {
                effective.insert(1, "--no-daemon".into());
                None
            } else {
                Some("Codex cannot confirm --no-daemon support; update Codex for reliable per-pane status. The original command remains available.")
            }
        }
    };
    (effective, notice)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn classify(args: &[&str]) -> Invocation {
        invocation(&args.iter().map(|s| (*s).into()).collect::<Vec<_>>())
    }
    #[test]
    fn native_interactive_arguments() {
        for args in [
            vec!["codex"],
            vec!["/tmp/codex", "resume", "session"],
            vec!["codex", "fork", "--last"],
            vec!["codex", "--model", "exec"],
            vec!["codex", "--config", "x=remote", "hello"],
            vec!["codex", "--", "exec"],
            vec!["codex", "--", "--remote"],
            vec!["codex", "--image", "a", "exec"],
            vec!["codex", "--image=a", "exec"],
            vec!["codex", "-ia", "exec"],
        ] {
            assert_eq!(
                classify(&args),
                Invocation::Local { isolated: false },
                "{args:?}"
            );
        }
        for args in [
            vec!["codex", "-mgpt-6.1"],
            vec!["codex", "-cfeatures.daemon_auto_start=false"],
            vec!["codex", "resume", "--last", "--include-non-interactive"],
        ] {
            assert_eq!(
                classify(&args),
                Invocation::Local { isolated: false },
                "{args:?}"
            );
        }
        assert_eq!(
            classify(&["codex", "resume", "--no-daemon", "--last"]),
            Invocation::Local { isolated: true }
        );
    }
    #[test]
    fn preserve_utilities_remote_and_unknown_options() {
        for command in [
            "exec",
            "e",
            "agents",
            "app-server",
            "queue",
            "review",
            "help",
            "--version",
            "--help",
        ] {
            assert_eq!(classify(&["codex", command]), Invocation::Unchanged);
            let original = vec!["codex".to_owned(), command.to_owned()];
            assert_eq!(prepare(&original, "/tmp"), (original, None));
        }
        assert_eq!(classify(&["sh", "-c", "codex"]), Invocation::Unchanged);
        assert_eq!(
            classify(&["codex", "--unknown", "exec"]),
            Invocation::Unsupported
        );
        assert_eq!(
            classify(&["codex", "--remote=unix:///tmp/server"]),
            Invocation::Remote
        );
        assert_eq!(
            classify(&["codex", "resume", "--remote", "unix:///tmp/server"]),
            Invocation::Remote
        );
        for args in [
            vec!["/missing/codex", "--unknown"],
            vec!["/missing/codex", "--remote=unix:///tmp/server"],
        ] {
            let original = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
            let (effective, notice) = prepare(&original, "/tmp");
            assert_eq!(effective, original);
            assert!(notice.is_some());
        }
    }
    #[test]
    fn explicit_isolation_preserves_argv_without_probe() {
        let original = vec![
            "/missing/codex".into(),
            "--no-daemon".into(),
            "resume".into(),
            "id".into(),
        ];
        assert_eq!(prepare(&original, "/tmp"), (original, None));
    }
}

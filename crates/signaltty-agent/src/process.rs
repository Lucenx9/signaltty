//! The program a pane is running, as screen-rule scoping sees it (spec 033):
//! the argv program's basename, or the script a language runtime runs.
//! Pure: callers read `/proc` or the spawn argv.

/// Basename of the program `argv` runs; for a runtime (`node`, `bun`, `deno`,
/// `python`, also `nodejs`) the script or module it runs (basename without extension);
/// inline code (`-c`, `-e`) keeps the runtime's own name.
pub fn process_name(argv: &[String]) -> String {
    let base = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    let Some(program) = argv.first().map(|p| base(p)) else {
        return String::new();
    };
    let runtime = matches!(
        program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.'),
        "node" | "nodejs" | "bun" | "deno" | "python"
    );
    if !runtime {
        return program;
    }
    let mut args = argv[1..].iter().map(String::as_str).peekable();
    // `bun run <file>` / `deno run <file>`: skip the subcommand only.
    if matches!(program.as_str(), "bun" | "deno") && args.peek() == Some(&"run") {
        args.next();
    }
    while let Some(arg) = args.next() {
        match arg {
            // Inline code: no script to name.
            "-c" | "-e" | "-p" | "--eval" | "--print" => break,
            // Flags whose value is a separate argument.
            "-r" | "--require" | "--import" | "--loader" | "-W" | "-X" => {
                args.next();
            }
            flag if flag.starts_with('-') => {}
            script => {
                let name = base(script);
                return match name.rsplit_once('.') {
                    Some((stem, _)) if !stem.is_empty() => stem.to_string(),
                    _ => name,
                };
            }
        }
    }
    program
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(argv: &[&str]) -> String {
        process_name(&argv.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn plain_programs_use_their_basename() {
        assert_eq!(name(&["gemini"]), "gemini");
        assert_eq!(name(&["/usr/local/bin/copilot", "--banner"]), "copilot");
        assert_eq!(name(&[]), "");
    }

    #[test]
    fn runtimes_name_the_script_they_run() {
        assert_eq!(
            name(&[
                "node",
                "/usr/lib/node_modules/@google/gemini-cli/bin/gemini"
            ]),
            "gemini"
        );
        assert_eq!(
            name(&[
                "/usr/bin/node",
                "--no-warnings",
                "/opt/droid/droid.js",
                "run"
            ]),
            "droid"
        );
        assert_eq!(name(&["bun", "run", "/x/kilo.ts"]), "kilo");
        assert_eq!(name(&["python3", "-u", "/srv/qodercli.py"]), "qodercli");
        // Debian and Ubuntu install Node.js as `nodejs`.
        assert_eq!(
            name(&["/usr/bin/nodejs", "/usr/lib/gemini/bin/gemini"]),
            "gemini"
        );
        // A runtime with nothing to run, or only flags, keeps its own name.
        assert_eq!(name(&["node"]), "node");
        assert_eq!(name(&["node", "--version"]), "node");
        // Shells are not unwrapped (spec 033 scope).
        assert_eq!(name(&["sh", "/tmp/gemini"]), "sh");
        // `run` is only bun/deno's subcommand, not any argument named run.
        assert_eq!(name(&["node", "run"]), "run");
        assert_eq!(name(&["bun", "run", "run"]), "run");
        assert_eq!(
            name(&["deno", "run", "--allow-all", "/x/qodercli.ts"]),
            "qodercli"
        );
        // Flags that take a value skip it; inline code names no script.
        assert_eq!(
            name(&["node", "-r", "ts-node/register", "/srv/droid.ts"]),
            "droid"
        );
        assert_eq!(
            name(&["node", "--require", "dotenv/config", "app.js"]),
            "app"
        );
        assert_eq!(name(&["python3", "-W", "ignore", "kilo.py"]), "kilo");
        assert_eq!(name(&["python3", "-c", "import x"]), "python3");
        assert_eq!(name(&["node", "-e", "1+1"]), "node");
        // `python -m pkg`: the module is the first plain argument.
        assert_eq!(name(&["python3", "-m", "qodercli", "--x"]), "qodercli");
    }
}

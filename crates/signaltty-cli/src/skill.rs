//! Gated agent skill: the shippable `SKILL.md` agents read from inside a
//! managed pane (directive 5). One source (`assets/SKILL.md`, embedded);
//! install writes it into the harness skills dirs with a marker so
//! uninstall only removes what install added. See ADR-0010 (deferred).

use std::path::{Path, PathBuf};

use crate::client::CliError;

pub const TEXT: &str = include_str!("../assets/SKILL.md");
pub const MARKER: &str = "<!-- signaltty-skill -->";

/// Harness skills dirs, rooted at `home` (mirrors `setup-agent.sh` homes).
fn skill_dirs(home: &Path) -> Vec<PathBuf> {
    [".agents/skills", ".claude/skills", ".codex/skills"]
        .iter()
        .map(|rel| home.join(rel).join("signaltty"))
        .collect()
}

fn skill_file(dir: &Path) -> PathBuf {
    dir.join("SKILL.md")
}

fn marked(text: &str) -> bool {
    text.lines().next().is_some_and(|l| l.trim() == MARKER)
}

/// Gate: agents act only from inside a managed pane.
pub fn check() -> Result<String, CliError> {
    match std::env::var("SIGNALTTY_PANE") {
        Ok(pane) if !pane.is_empty() => Ok(pane),
        _ => Err(CliError::Usage(
            "not inside a managed pane (SIGNALTTY_PANE is unset); plain terminal work is fine, but the agent API needs a pane".to_string(),
        )),
    }
}

pub fn install(home: &Path, json: bool) -> Result<(), CliError> {
    let mut changed = false;
    let mut files = Vec::new();
    for dir in skill_dirs(home) {
        let path = skill_file(&dir);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        files.push(path.display().to_string());
        if current == TEXT {
            continue;
        }
        // Foreign (unmarked) content must survive; outdated installs we
        // previously wrote (marker line present) are refreshed to TEXT.
        if !current.is_empty() && !marked(&current) {
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|e| CliError::Io(e.to_string()))?;
        std::fs::write(&path, TEXT).map_err(|e| CliError::Io(e.to_string()))?;
        changed = true;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"files": files, "changed": changed})
        );
    } else if changed {
        for f in &files {
            println!("installed skill → {f}");
        }
    } else {
        println!("skill already installed");
    }
    Ok(())
}

pub fn uninstall(home: &Path, json: bool) -> Result<(), CliError> {
    let mut changed = false;
    let mut files = Vec::new();
    for dir in skill_dirs(home) {
        let path = skill_file(&dir);
        files.push(path.display().to_string());
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current.is_empty() || !marked(&current) {
            continue; // foreign file (or absent): never touch it
        }
        std::fs::remove_file(&path).map_err(|e| CliError::Io(e.to_string()))?;
        // Prune our dir when empty; never the parent skills dir.
        std::fs::remove_dir(&dir).ok();
        changed = true;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"files": files, "changed": changed})
        );
    } else if changed {
        println!("uninstalled skill");
    } else {
        println!("skill not installed");
    }
    Ok(())
}

pub fn status(home: &Path, json: bool) -> Result<(), CliError> {
    let mut entries = Vec::new();
    for dir in skill_dirs(home) {
        let path = skill_file(&dir);
        let installed = std::fs::read_to_string(&path)
            .map(|t| t == TEXT)
            .unwrap_or(false);
        entries.push(serde_json::json!({
            "dir": dir.display().to_string(), "installed": installed,
        }));
    }
    if json {
        println!("{}", serde_json::json!({"skills": entries}));
    } else {
        for e in &entries {
            println!(
                "{}: {}",
                e["dir"].as_str().unwrap_or("?"),
                if e["installed"].as_bool().unwrap() {
                    "installed"
                } else {
                    "not installed"
                }
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_carries_marker_and_gate_env() {
        assert!(marked(TEXT), "asset must start with the marker line");
        assert!(TEXT.contains("SIGNALTTY_PANE"));
        assert!(TEXT.contains("signaltty schema"));
    }

    #[test]
    fn check_follows_signaltty_pane() {
        std::env::remove_var("SIGNALTTY_PANE");
        assert!(check().is_err());
        std::env::set_var("SIGNALTTY_PANE", "pane_x");
        assert_eq!(check().unwrap(), "pane_x");
        std::env::remove_var("SIGNALTTY_PANE");
    }

    fn temp_home(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "signaltty-skill-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_refreshes_outdated_marked_skill_and_spares_foreign() {
        let home = temp_home("refresh");
        let agents = home.join(".agents/skills/signaltty");
        std::fs::create_dir_all(&agents).unwrap();
        let skill = agents.join("SKILL.md");
        // Previously installed, but not equal to the current embedded text.
        std::fs::write(&skill, format!("{MARKER}\noutdated body\n")).unwrap();
        assert!(marked(&std::fs::read_to_string(&skill).unwrap()));
        assert_ne!(std::fs::read_to_string(&skill).unwrap(), TEXT);

        install(&home, true).unwrap();
        assert_eq!(std::fs::read_to_string(&skill).unwrap(), TEXT);

        // Foreign content without our marker must not be overwritten.
        let foreign_home = temp_home("foreign");
        let foreign_dir = foreign_home.join(".agents/skills/signaltty");
        std::fs::create_dir_all(&foreign_dir).unwrap();
        let foreign = foreign_dir.join("SKILL.md");
        std::fs::write(&foreign, "mine\n").unwrap();
        install(&foreign_home, true).unwrap();
        assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "mine\n");

        std::fs::remove_dir_all(&home).ok();
        std::fs::remove_dir_all(&foreign_home).ok();
    }
}

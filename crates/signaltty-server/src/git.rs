//! Lightweight Git context: root, name, branch, detached, dirty.
//! Explicit refreshes only — never aggressive polling. See docs §17.

use std::process::Command;

use signaltty_core::model::GitInfo;

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn git_info(cwd: &str) -> GitInfo {
    let root = git(cwd, &["rev-parse", "--show-toplevel"]);
    if root.is_none() {
        return GitInfo::default();
    }
    let root = root.unwrap();
    let name = std::path::Path::new(&root)
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    let branch = git(cwd, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let detached = branch.is_none();
    // Untracked excluded: cheap enough for explicit refresh.
    let dirty =
        git(cwd, &["status", "--porcelain=v1", "--untracked-files=no"]).map(|s| !s.is_empty());
    GitInfo {
        root: Some(root),
        name,
        branch,
        detached,
        dirty,
    }
}

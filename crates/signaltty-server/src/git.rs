//! Lightweight Git context: root, name, branch, detached, dirty.
//! Explicit refreshes only — never aggressive polling. See docs §17.

use std::process::Command;

use signaltty_core::model::GitInfo;

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let text = String::from_utf8(git_bytes(cwd, args)?).ok()?;
    Some(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

fn git_bytes(cwd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(out.stdout)
}

/// One changed file in a worktree diff. Untracked files carry no counts;
/// binary files carry zero counts with `binary` set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DiffFile {
    pub path: String,
    pub added: u64,
    pub removed: u64,
    pub untracked: bool,
    pub binary: bool,
}

/// Per-directory rollup of tracked counts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DiffDir {
    pub dir: String,
    pub added: u64,
    pub removed: u64,
}

/// Worktree-vs-HEAD diff as data (t3code `+N −N` language). Not
/// turn-scoped: git has no snapshot of turn start (see ADR-0011).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorktreeDiff {
    pub branch: Option<String>,
    pub files: Vec<DiffFile>,
    pub dirs: Vec<DiffDir>,
    pub added: u64,
    pub removed: u64,
}

/// `git diff --numstat HEAD` + untracked names, or `None` outside a repo.
/// On demand only — never polled, never snapshotted.
pub fn git_diff(cwd: &str) -> Option<WorktreeDiff> {
    let root = git(cwd, &["rev-parse", "--show-toplevel"])?;
    let cwd = root.as_str();
    let branch = git(cwd, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let base = git(cwd, &["rev-parse", "--verify", "HEAD"])
        .or_else(|| git(cwd, &["hash-object", "-t", "tree", "--stdin"]))?;
    let numstat = git_bytes(
        cwd,
        &[
            "diff",
            "--numstat",
            "-z",
            "--no-renames",
            "--no-relative",
            &base,
        ],
    )?;
    let mut files = Vec::new();
    for record in numstat.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let line = std::str::from_utf8(record).ok()?;
        let mut parts = line.splitn(3, '\t');
        let (added_s, removed_s, path) = match (parts.next(), parts.next(), parts.next()) {
            (Some(a), Some(r), Some(p)) => (a, r, p),
            _ => continue,
        };
        let binary = added_s == "-";
        files.push(DiffFile {
            path: path.to_string(),
            added: added_s.parse().unwrap_or(0),
            removed: removed_s.parse().unwrap_or(0),
            untracked: false,
            binary,
        });
    }
    let status = git_bytes(
        cwd,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--full-name",
            "-z",
        ],
    )
    .unwrap_or_default();
    for path in status.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        files.push(DiffFile {
            path: std::str::from_utf8(path).ok()?.to_owned(),
            added: 0,
            removed: 0,
            untracked: true,
            binary: false,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut dir_sums: std::collections::BTreeMap<String, (u64, u64)> =
        std::collections::BTreeMap::new();
    let mut added = 0u64;
    let mut removed = 0u64;
    for f in &files {
        if f.untracked {
            continue;
        }
        added += f.added;
        removed += f.removed;
        let dir = std::path::Path::new(&f.path)
            .parent()
            .map(|p| {
                let s = p.to_string_lossy().to_string();
                if s.is_empty() {
                    ".".to_string()
                } else {
                    s
                }
            })
            .unwrap_or_else(|| ".".to_string());
        let entry = dir_sums.entry(dir).or_insert((0, 0));
        entry.0 += f.added;
        entry.1 += f.removed;
    }
    let dirs = dir_sums
        .into_iter()
        .map(|(dir, (added, removed))| DiffDir {
            dir,
            added,
            removed,
        })
        .collect();
    Some(WorktreeDiff {
        branch,
        files,
        dirs,
        added,
        removed,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_repo(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "signaltty-diff-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("a.txt"), "1\n2\n3\n").unwrap();
        std::fs::write(dir.join("sub/b.txt"), "x\n").unwrap();
        // Binary file: 8 bytes with a NUL.
        std::fs::write(dir.join("bin.dat"), [0x00, 0x01, 0x02, 0x03]).unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "base"]);
        // Modify: +3/−1 on a.txt; append sub/b.txt (+1).
        std::fs::write(dir.join("a.txt"), "1\n2 changed\n3\n4\n5\n").unwrap();
        std::fs::write(dir.join("sub/b.txt"), "x\ny\n").unwrap();
        std::fs::write(dir.join("bin.dat"), [0x00, 0x01, 0x02, 0x03, 0x04]).unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        dir
    }

    #[test]
    fn diff_reports_numstat_untracked_and_binary() {
        let dir = fixture_repo("basic");
        let d = git_diff(dir.to_str().unwrap()).unwrap();
        let file = |p: &str| d.files.iter().find(|f| f.path == p).unwrap().clone();
        // a.txt: lines 2 changed (1 del + 1 add?) — assert against git itself.
        let raw = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["diff", "--numstat", "HEAD"])
            .output()
            .unwrap();
        let text = String::from_utf8(raw.stdout).unwrap();
        for line in text.lines() {
            let mut parts = line.split('\t');
            let (a, r, p) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            if a == "-" {
                let f = file(p);
                assert!(f.binary && f.added == 0 && f.removed == 0, "{p}");
            } else {
                let f = file(p);
                assert_eq!(
                    (f.added, f.removed),
                    (a.parse().unwrap(), r.parse().unwrap()),
                    "{p}"
                );
            }
        }
        let new = file("new.txt");
        assert!(new.untracked && !new.binary);
        assert!(file("bin.dat").binary);
        // Dir rollup covers tracked counts only, sorted deterministically.
        let total_dirs: (u64, u64) = d
            .dirs
            .iter()
            .map(|g| (g.added, g.removed))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        assert_eq!((d.added, d.removed), total_dirs);
        assert!(d.dirs.windows(2).all(|w| w[0].dir <= w[1].dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn diff_outside_a_repo_is_none() {
        assert_eq!(git_diff("/tmp"), None);
    }

    #[test]
    fn diff_preserves_tabs_and_newlines_in_filenames() {
        let dir = fixture_repo("names");
        let tracked = "tracked\tfile\nname.txt";
        let untracked = "new\tfile\nname.txt";
        std::fs::write(dir.join(tracked), "old\n").unwrap();
        for args in [
            vec!["add", "--", tracked],
            vec!["commit", "-qm", "unusual filename"],
        ] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(dir.join(tracked), "new\nextra\n").unwrap();
        std::fs::write(dir.join(untracked), "untracked\n").unwrap();
        let diff = git_diff(dir.to_str().unwrap()).unwrap();
        let file = diff
            .files
            .iter()
            .find(|f| f.path == tracked)
            .expect("literal tracked filename");
        assert_eq!((file.added, file.removed), (2, 1));
        assert!(diff
            .files
            .iter()
            .any(|f| f.path == untracked && f.untracked));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn diff_in_unborn_repo_reports_staged_and_untracked_files() {
        let dir = fixture_repo("unborn");
        std::fs::remove_dir_all(dir.join(".git")).unwrap();
        assert!(Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["add", "a.txt"])
            .status()
            .unwrap()
            .success());
        let diff = git_diff(dir.to_str().unwrap()).expect("unborn repo has a working-tree diff");
        let staged = diff.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!((staged.added, staged.removed), (5, 0));
        assert!(diff
            .files
            .iter()
            .any(|f| f.path == "new.txt" && f.untracked));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

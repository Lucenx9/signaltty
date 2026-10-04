//! Lightweight Git context: root, name, branch, detached, dirty.
//! Explicit refreshes only — never aggressive polling. See docs §17.

use std::process::Command;

pub fn git_output(cwd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("git").arg("-C").arg(cwd).args(args).output()
}

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

/// Diff task worktree vs recorded `base_sha`.
/// Tracked changes via `git diff --numstat base_sha` + untracked files via `ls-files`
/// with line count as additions.
/// Does NOT mutate git index (no `git add -N`).
pub fn task_git_diff(
    worktree_path: &str,
    base_sha: &str,
) -> Result<WorktreeDiff, (String, String)> {
    let root = git(worktree_path, &["rev-parse", "--show-toplevel"]).ok_or_else(|| {
        (
            signaltty_proto::code::IO_ERROR.into(),
            format!("not a git repo: {worktree_path}"),
        )
    })?;
    let cwd = root.as_str();
    let branch = git(cwd, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let verify = git(cwd, &["rev-parse", "--verify", base_sha]).ok_or_else(|| {
        (
            signaltty_proto::code::IO_ERROR.into(),
            format!("missing base commit: {base_sha}"),
        )
    })?;
    let base = verify.trim();

    let numstat = git_bytes(
        cwd,
        &[
            "diff",
            "--numstat",
            "-z",
            "--no-renames",
            "--no-relative",
            base,
        ],
    )
    .ok_or_else(|| {
        (
            signaltty_proto::code::IO_ERROR.into(),
            "git diff failed".into(),
        )
    })?;

    let mut files = Vec::new();
    for record in numstat.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let line = match std::str::from_utf8(record) {
            Ok(l) => l,
            Err(_) => continue,
        };
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

    for path_bytes in status.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let full_path = std::path::Path::new(cwd).join(path_str);
        // Capped, non-following read. A symlink, fifo, or oversized file is
        // binary with no line count so the runtime never loads the body.
        let (added, binary) = untracked_line_stats(&full_path);
        files.push(DiffFile {
            path: path_str.to_string(),
            added,
            removed: 0,
            untracked: true,
            binary,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut dir_sums: std::collections::BTreeMap<String, (u64, u64)> =
        std::collections::BTreeMap::new();
    let mut added = 0u64;
    let mut removed = 0u64;
    for f in &files {
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

    Ok(WorktreeDiff {
        branch,
        files,
        dirs,
        added,
        removed,
    })
}

/// Line count for one untracked path. Never follows symlinks, never reads
/// past [`signaltty_core::diff::MAX_PREVIEW_BYTES`], and opens non-blocking
/// so a fifo cannot stall the caller. Anything that is not a small regular
/// text file is `(0, true)`.
fn untracked_line_stats(path: &std::path::Path) -> (u64, bool) {
    use std::io::Read;
    use std::os::fd::FromRawFd;

    let bytes = match path.as_os_str().to_str() {
        Some(s) => s.as_bytes(),
        None => return (0, true),
    };
    let c_path = match std::ffi::CString::new(bytes) {
        Ok(c) => c,
        Err(_) => return (0, true),
    };
    let fd = unsafe {
        nix::libc::open(
            c_path.as_ptr(),
            nix::libc::O_RDONLY
                | nix::libc::O_NOFOLLOW
                | nix::libc::O_NONBLOCK
                | nix::libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return (0, true);
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let meta = match file.metadata() {
        Ok(meta) => meta,
        Err(_) => return (0, true),
    };
    let cap = signaltty_core::diff::MAX_PREVIEW_BYTES as u64;
    if !meta.is_file() || meta.len() > cap {
        return (0, true);
    }
    let mut body = Vec::new();
    if file.take(cap + 1).read_to_end(&mut body).is_err() || body.len() as u64 > cap {
        return (0, true);
    }
    if body.contains(&0) {
        return (0, true);
    }
    if body.is_empty() {
        return (0, false);
    }
    let mut lines = body.split(|b| *b == b'\n').count() as u64;
    if body.ends_with(b"\n") {
        lines = lines.saturating_sub(1);
    }
    (lines, false)
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

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

/// Strict porcelain cleanliness check: `Ok(true)` = clean tree,
/// `Ok(false)` = dirty tree. A git failure (spawn error or non-zero exit,
/// e.g. dubious ownership or a missing `.git` pointer) is `Err` — an empty
/// or missing stdout is never treated as clean.
pub fn porcelain_clean(cwd: &str, args: &[&str]) -> Result<bool, String> {
    let out = git_output(cwd, args).map_err(|e| format!("git failed: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        };
        return Err(format!(
            "git {} exited {}{detail}",
            args.join(" "),
            out.status
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// True when `path` is a registered worktree of `repo` (herdr leftover
/// guard: a leftover directory may be force-deleted only after git says it
/// is not a registered worktree). Fail-closed: when git itself fails, the
/// path counts as registered so callers never `remove_dir_all` blind.
pub fn is_worktree_registered(repo: &str, path: &str) -> bool {
    let out = match git_output(repo, &["worktree", "list", "--porcelain"]) {
        Ok(out) if out.status.success() => out,
        _ => return true,
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let canon = std::path::Path::new(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .ok();
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|w| w == path || canon.as_deref().is_some_and(|c| w == c))
}

/// Resolve base_ref into a 40-character commit SHA. A best-effort
/// `git fetch origin` is the caller's job (bounded, async) — see `task.start`.
pub fn resolve_base_ref(repo: &str, base_ref: &str) -> Result<String, String> {
    let ref_spec = format!("{base_ref}^{{commit}}");
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", &ref_spec])
        .output()
        .map_err(|e| format!("git rev-parse failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cannot resolve base_ref '{base_ref}' in repo '{repo}'"
        ));
    }
    let sha = String::from_utf8(out.stdout)
        .map_err(|e| format!("invalid utf-8 in rev-parse output: {e}"))?
        .trim()
        .to_string();
    if sha.len() != 40 {
        return Err(format!("expected 40-char SHA, got '{sha}'"));
    }
    Ok(sha)
}

/// Detect the currently checked-out branch in repo.
/// Returns None if HEAD is detached or outside a branch.
pub fn detect_target_branch(repo: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "--show-current"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let branch = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// Check if a local branch exists in the repository.
pub fn branch_exists(repo: &str, branch: &str) -> bool {
    let ref_spec = format!("refs/heads/{branch}");
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", &ref_spec])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Sanitize branch name for use as a directory component (slashes -> dashes, etc.).
pub fn sanitize_branch_for_path(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Compute default worktree path under $XDG_DATA_HOME/signaltty/worktrees/<repo-name>-<short-hash>/<sanitized-branch>.
pub fn default_worktree_path(repo: &str, branch: &str) -> PathBuf {
    let repo_path = Path::new(repo);
    let canonical = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());
    let repo_dirname = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    let mut hasher = DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    let short_hash = format!("{:08x}", (hasher.finish() as u32));
    let sanitized_branch = sanitize_branch_for_path(branch);

    signaltty_core::paths::data_dir()
        .join("worktrees")
        .join(format!("{repo_dirname}-{short_hash}"))
        .join(sanitized_branch)
}

/// Validate a branch name per `git check-ref-format --branch`.
/// Rejects empty string, leading '-', any name containing `refs/` or `@{`,
/// and any name whose `--branch` expansion differs from the input (e.g.
/// `@{-1}` expands to the previous branch; `refs/heads/<current>` would
/// otherwise create a nested ref whose commits a later merge silently drops).
/// Callers must still pass `--` before the branch in git invocations.
pub fn validate_branch_name(branch: &str) -> Result<(), String> {
    if branch.is_empty() || branch.starts_with('-') {
        return Err(format!(
            "invalid branch name '{branch}': cannot start with '-' or be empty"
        ));
    }
    if branch.contains("refs/") || branch.contains("@{") {
        return Err(format!(
            "invalid branch name '{branch}': must not contain 'refs/' or '@{{'"
        ));
    }
    let out = Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .output()
        .map_err(|e| format!("git check-ref-format failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "invalid branch name '{branch}' per git check-ref-format"
        ));
    }
    let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if printed != branch {
        return Err(format!(
            "invalid branch name '{branch}': git expands it to '{printed}'"
        ));
    }
    Ok(())
}

/// True when `HEAD` of `repo` descends from `tip` (a full commit SHA).
/// Used after `task.finish --merge` to prove the task commits landed
/// before recording `merged`.
pub fn head_contains_branch_tip(repo: &str, tip: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--is-ancestor", tip, "HEAD"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Validate a base ref format.
/// Rejects empty string, leading '-', or names invalid per git check-ref-format.
pub fn validate_base_ref_format(base_ref: &str) -> Result<(), String> {
    if base_ref.is_empty() || base_ref.starts_with('-') {
        return Err(format!(
            "invalid base_ref '{base_ref}': cannot start with '-' or be empty"
        ));
    }
    // 40-character hex commit SHA is valid
    if base_ref.len() == 40 && base_ref.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    let out = Command::new("git")
        .args(["check-ref-format", "--allow-onelevel", base_ref])
        .output()
        .map_err(|e| format!("git check-ref-format failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "invalid base_ref '{base_ref}' per git check-ref-format"
        ));
    }
    Ok(())
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

    #[test]
    fn branch_validation_rejects_expanding_and_ref_paths() {
        let dir = fixture_repo("branch-names");
        let dir_str = dir.to_str().unwrap();
        let current = detect_target_branch(dir_str).unwrap();
        // Plain names pass.
        assert!(validate_branch_name("feature/work").is_ok());
        assert!(validate_branch_name(&current).is_ok());
        // `git check-ref-format --branch` exits 0 for these but must not
        // be stored: refs/heads/<current> would create a nested ref whose
        // commits a later merge (resolving to the checked-out branch)
        // silently drops.
        assert!(validate_branch_name(&format!("refs/heads/{current}")).is_err());
        assert!(validate_branch_name("refs/heads/other").is_err());
        assert!(validate_branch_name("task/refs/x").is_err());
        // `@{-1}` expands via stdout; stdout must equal the input.
        assert!(validate_branch_name("@{-1}").is_err());
        assert!(validate_branch_name("foo@{").is_err());
        assert!(validate_branch_name("foo@{u}").is_err());
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("-leading-dash").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn head_descends_from_branch_tip_after_merge() {
        let dir = fixture_repo("descendant");
        let dir_str = dir.to_str().unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
            out
        };
        let base = detect_target_branch(dir_str).unwrap();
        run(&["checkout", "-qb", "task-x"]);
        std::fs::write(dir.join("t.txt"), "work\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "work"]);
        let tip_out = run(&["rev-parse", "task-x"]);
        let tip = String::from_utf8(tip_out.stdout)
            .unwrap()
            .trim()
            .to_string();
        run(&["checkout", "-q", &base]);
        // Before merge the base HEAD does not contain the tip.
        assert!(!head_contains_branch_tip(dir_str, &tip));
        run(&["merge", "--no-ff", "--no-edit", "--", "task-x"]);
        assert!(head_contains_branch_tip(dir_str, &tip));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn untracked_symlink_oversize_and_fifo_are_not_loaded() {
        let dir = fixture_repo("untracked-cap");
        let dir_str = dir.to_str().unwrap();
        let head = String::from_utf8(
            Command::new("git")
                .args(["-C", dir_str, "rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let head = head.trim();
        std::fs::write(dir.join("note.txt"), "a\nb\n").unwrap();
        std::os::unix::fs::symlink("note.txt", dir.join("link.txt")).unwrap();
        std::fs::write(dir.join("big.txt"), vec![b'a'; 512 * 1024 + 1]).unwrap();
        // git ls-files does not list a fifo, but it does list a symlink to one.
        // Following that symlink is what blocks `std::fs::read`.
        let pipe = dir.join("pipe");
        let c_pipe = std::ffi::CString::new(pipe.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { nix::libc::mkfifo(c_pipe.as_ptr(), 0o644) }, 0);
        std::os::unix::fs::symlink("pipe", dir.join("pipe-link")).unwrap();

        let started = std::time::Instant::now();
        let direct = untracked_line_stats(&pipe);
        assert_eq!(direct, (0, true), "fifo must not be read, got {direct:?}");
        let diff = task_git_diff(dir_str, head).unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "task diff blocked while classifying untracked files"
        );
        let file = |p: &str| {
            diff.files
                .iter()
                .find(|f| f.path == p)
                .unwrap_or_else(|| panic!("missing {p} in {diff:?}"))
                .clone()
        };
        let note = file("note.txt");
        assert!(note.untracked && !note.binary && note.added == 2);
        let link = file("link.txt");
        assert!(
            link.untracked && link.binary && link.added == 0,
            "symlink must not be followed, got {link:?}"
        );
        let big = file("big.txt");
        assert!(
            big.untracked && big.binary && big.added == 0,
            "oversize untracked file must not be counted, got {big:?}"
        );
        let fifo = file("pipe-link");
        assert!(
            fifo.untracked && fifo.binary && fifo.added == 0,
            "symlink to a fifo must not be followed, got {fifo:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn porcelain_failure_is_error_never_clean() {
        // A failed `git status` (missing .git, dubious ownership, corrupt
        // index) must surface as Err: empty stdout is not a clean tree.
        let outside = std::env::temp_dir().join(format!(
            "signaltty-norepo-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        // A `.git` pointer to nowhere fails status even when this directory
        // sits inside another repo (test TMPDIR often does). Empty stdout
        // from that failure must not look clean.
        std::fs::write(outside.join(".git"), "gitdir: /no/such/gitdir\n").unwrap();
        let outside_str = outside.to_str().unwrap();
        assert!(
            porcelain_clean(outside_str, &["status", "--porcelain"]).is_err(),
            "broken gitdir must not count as a clean tree"
        );
        let dir = fixture_repo("porcelain");
        let dir_str = dir.to_str().unwrap();
        // Fixture has modifications -> dirty.
        assert_eq!(
            porcelain_clean(dir_str, &["status", "--porcelain"]),
            Ok(false)
        );
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn worktree_registration_guard() {
        let dir = fixture_repo("registered");
        let dir_str = dir.to_str().unwrap();
        let wt = dir.join("wt-guard");
        let wt_str = wt.to_str().unwrap().to_string();
        assert!(!is_worktree_registered(dir_str, &wt_str));
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["worktree", "add", &wt_str])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(is_worktree_registered(dir_str, &wt_str));
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["worktree", "remove", "--force", &wt_str])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(!is_worktree_registered(dir_str, &wt_str));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn base_ref_resolution_and_branch_helpers() {
        let dir = fixture_repo("helpers");
        let dir_str = dir.to_str().unwrap();

        // resolve_base_ref
        let head_sha = resolve_base_ref(dir_str, "HEAD").unwrap();
        assert_eq!(head_sha.len(), 40);

        // bad base_ref
        assert!(resolve_base_ref(dir_str, "no-such-ref-12345").is_err());

        // branch_exists
        let current_branch = detect_target_branch(dir_str).unwrap();
        assert!(branch_exists(dir_str, &current_branch));
        assert!(!branch_exists(dir_str, "nonexistent-branch"));

        // sanitize_branch_for_path
        assert_eq!(
            sanitize_branch_for_path("feature/my-task:v1"),
            "feature-my-task-v1"
        );

        // default_worktree_path
        let wt = default_worktree_path(dir_str, "feature/foo");
        let wt_str = wt.to_string_lossy();
        assert!(wt_str.contains("worktrees"));
        assert!(wt_str.contains("feature-foo"));

        std::fs::remove_dir_all(dir).ok();
    }
}

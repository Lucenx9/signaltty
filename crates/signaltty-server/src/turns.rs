//! Turn baselines (spec 041, ADR-0029): a Git tree of the workspace checkout
//! captured when an agent turn starts, so review can scope to that turn.
//! Capture writes only Git objects: never the user's index, refs or files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use signaltty_proto::{code, event};
use tokio::sync::broadcast;

use crate::params::{bad_params, ParamError};
use crate::router::Ctx;
use crate::store::StoredEvent;

const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct Baseline {
    pub pane_id: String,
    pub started_at: DateTime<Utc>,
    /// The checkout's tree at turn start, or why capturing it failed.
    pub tree: Result<String, String>,
    /// The `agent.working` event that started the turn: newest wins.
    seq: u64,
}

/// The latest turn baseline per workspace. Runtime only: a restarted
/// server has no turn until the next one starts.
#[derive(Default)]
pub struct TurnBaselines(Mutex<HashMap<String, Baseline>>);

impl TurnBaselines {
    pub fn get(&self, workspace_id: &str) -> Option<Baseline> {
        self.0.lock().unwrap().get(workspace_id).cloned()
    }

    pub fn forget(&self, workspace_id: &str) {
        self.0.lock().unwrap().remove(workspace_id);
    }

    /// Captures can finish out of order; an older turn never replaces a newer one.
    fn record(&self, workspace_id: &str, baseline: Baseline) -> bool {
        let mut map = self.0.lock().unwrap();
        if map.get(workspace_id).is_some_and(|b| b.seq >= baseline.seq) {
            return false;
        }
        map.insert(workspace_id.to_owned(), baseline);
        true
    }
}

/// A tree of the checkout and the root its paths are relative to.
pub struct Snapshot {
    pub root: String,
    pub tree: String,
}

/// A private index in a fresh 0700 directory, so another local user cannot
/// pre-create or symlink the predictable path in the shared temp dir. The
/// directory (with any lock git leaves) is removed on every path.
struct TempIndex {
    dir: PathBuf,
    index: PathBuf,
}

impl TempIndex {
    fn create() -> std::io::Result<TempIndex> {
        use std::os::unix::fs::DirBuilderExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut attempts = 0;
        loop {
            let dir = std::env::temp_dir().join(format!(
                "signaltty-turn-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            // mkdir never follows a symlink and fails if the path exists.
            match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => {
                    let index = dir.join("index");
                    return Ok(TempIndex { dir, index });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempts < 16 => {
                    attempts += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Tree of every tracked and unignored untracked file in `cwd`'s checkout,
/// staged through a copy of the index so the user's index stays untouched.
pub async fn snapshot(cwd: &str) -> Result<Snapshot, ParamError> {
    let root = git(cwd, &["rev-parse", "--show-toplevel"], None).await?;
    let index = git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        None,
    )
    .await?;
    let temp = TempIndex::create().map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
    // The copy keeps stat data, so only changed files are hashed again.
    match std::fs::copy(&index, &temp.index) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err((code::IO_ERROR.into(), e.to_string())),
    }
    git(&root, &["add", "--all"], Some(&temp.index)).await?;
    let tree = git(&root, &["write-tree"], Some(&temp.index)).await?;
    Ok(Snapshot { root, tree })
}

/// The checkout root of `cwd`; a non-repo is `BAD_PARAMS`, as in the HEAD
/// scope, even before any turn is recorded.
pub async fn repo_root(cwd: &str) -> Result<String, ParamError> {
    git(cwd, &["rev-parse", "--show-toplevel"], None)
        .await
        .map_err(|(c, e)| {
            if c == code::BAD_PARAMS {
                bad_params(format!("not a git repo: {cwd}"))
            } else {
                (c, e)
            }
        })
}

/// Changed files between two trees of one checkout.
pub async fn changed_files(
    root: &str,
    base: &str,
    current: &str,
) -> Result<Vec<crate::git::DiffFile>, ParamError> {
    // Raw bytes: `numstat_files` skips non-UTF-8 paths itself.
    let numstat = git_raw(
        root,
        &[
            "diff",
            "--numstat",
            "-z",
            "--no-renames",
            "--no-relative",
            base,
            current,
        ],
        None,
    )
    .await?;
    Ok(crate::git::numstat_files(&numstat))
}

/// The tree a turn-scoped read compares against, or `None` before any turn.
pub fn baseline_tree(baseline: &Baseline) -> Result<&str, ParamError> {
    baseline.tree.as_deref().map_err(|e| {
        (
            code::IO_ERROR.into(),
            format!("The latest turn's baseline could not be captured: {e}"),
        )
    })
}

pub fn require(ctx: &Ctx, workspace_id: &str) -> Result<Baseline, ParamError> {
    ctx.turns
        .get(workspace_id)
        .ok_or_else(|| bad_params("No agent turn recorded in this workspace yet"))
}

async fn git(cwd: &str, args: &[&str], index: Option<&Path>) -> Result<String, ParamError> {
    let text = String::from_utf8(git_raw(cwd, args, index).await?)
        .map_err(|_| bad_params("Git output is not valid UTF-8"))?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

/// Stdout of a Git command as bytes, for output that may carry paths.
async fn git_raw(cwd: &str, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>, ParamError> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = tokio::time::timeout(SNAPSHOT_TIMEOUT, command.output())
        .await
        .map_err(|_| (code::TIMEOUT.into(), "Git snapshot timed out".into()))?
        .map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
    if !output.status.success() {
        return Err(bad_params(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(output.stdout)
}

/// A turn starts on `agent.working`, except when a permission prompt
/// (`blocked`) hands back to the same turn.
fn turn_start(ev: &StoredEvent) -> Option<&str> {
    (ev.name == event::AGENT_WORKING && ev.payload["prev"] != "blocked")
        .then(|| ev.payload["pane_id"].as_str())
        .flatten()
}

pub fn spawn_capture(ctx: Arc<Ctx>) -> tokio::task::JoinHandle<()> {
    let (mut rx, mut last_seq) = {
        let store = ctx.store.read().unwrap();
        (ctx.bcast.subscribe(), store.seq)
    };
    tokio::spawn(async move {
        loop {
            let events = match rx.recv().await {
                Ok(ev) => vec![ev],
                // Lifecycle events stay in the Store ring; pty.data does not.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    ctx.store.read().unwrap().events_since(last_seq)
                }
                Err(broadcast::error::RecvError::Closed) => return,
            };
            for ev in events {
                if ev.seq <= last_seq {
                    continue;
                }
                last_seq = ev.seq;
                if let Some(pane_id) = turn_start(&ev) {
                    tokio::spawn(capture(ctx.clone(), pane_id.to_owned(), ev.seq));
                }
            }
        }
    })
}

async fn capture(ctx: Arc<Ctx>, pane_id: String, seq: u64) {
    let target = {
        let s = ctx.store.read().unwrap();
        s.panes
            .get(&pane_id)
            .and_then(|p| s.workspaces.get(&p.workspace_id))
            .map(|ws| (ws.id.clone(), ws.cwd.clone()))
    };
    let Some((workspace_id, cwd)) = target else {
        return;
    };
    let started_at = Utc::now();
    let tree = snapshot(&cwd).await.map(|s| s.tree).map_err(|(_, e)| e);
    if let Err(e) = &tree {
        tracing::debug!("turn baseline for {workspace_id}: {e}");
    }
    // Record under the Store lock, so a concurrent workspace.close (which
    // forgets the baseline under the same lock) cannot be undone.
    let mut s = ctx.store.write().unwrap();
    if !s.workspaces.contains_key(&workspace_id) {
        return;
    }
    let baseline = Baseline {
        pane_id: pane_id.clone(),
        started_at,
        tree,
        seq,
    };
    // A failed capture is still the latest turn: reads report its error
    // instead of showing an older turn.
    if ctx.turns.record(&workspace_id, baseline) {
        s.emit(
            event::WORKSPACE_TURN_STARTED,
            json!({"workspace_id": workspace_id, "pane_id": pane_id, "started_at": started_at}),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working(prev: &str) -> StoredEvent {
        StoredEvent {
            seq: 1,
            name: event::AGENT_WORKING.into(),
            payload: json!({"pane_id": "p", "lifecycle": "working", "prev": prev}),
        }
    }

    #[test]
    fn a_turn_starts_on_working_unless_resuming_from_blocked() {
        assert_eq!(turn_start(&working("idle")), Some("p"));
        assert_eq!(turn_start(&working("done")), Some("p"));
        assert_eq!(turn_start(&working("blocked")), None);
        let mut done = working("working");
        done.name = event::AGENT_DONE.into();
        assert_eq!(turn_start(&done), None);
    }

    #[test]
    fn an_older_capture_never_replaces_a_newer_turn() {
        let turns = TurnBaselines::default();
        let at = |seq| Baseline {
            pane_id: "p".into(),
            started_at: Utc::now(),
            tree: Ok(format!("tree{seq}")),
            seq,
        };
        assert!(turns.record("ws", at(5)));
        assert!(!turns.record("ws", at(3)));
        assert_eq!(turns.get("ws").unwrap().tree.unwrap(), "tree5");
        turns.forget("ws");
        assert!(turns.get("ws").is_none());
    }

    #[tokio::test]
    async fn snapshot_includes_untracked_files_and_leaves_the_index_alone() {
        let repo = signaltty_testkit::TempGitRepo::new();
        let cwd = repo.path().to_str().unwrap();
        std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
        let status =
            |repo: &signaltty_testkit::TempGitRepo| repo.git(&["status", "--porcelain"]).stdout;
        let before = status(&repo);
        let snap = snapshot(cwd).await.unwrap();
        assert_eq!(status(&repo), before, "index untouched");
        let listed = repo.git(&["ls-tree", "--name-only", &snap.tree]).stdout;
        assert_eq!(String::from_utf8(listed).unwrap(), "README.md\nnew.txt\n");
    }

    #[tokio::test]
    async fn changed_files_skips_non_utf8_paths_instead_of_failing() {
        use std::os::unix::ffi::OsStrExt;
        let repo = signaltty_testkit::TempGitRepo::new();
        let cwd = repo.path().to_str().unwrap();
        // A Latin-1 name (`caf\xe9.txt`) is a valid Linux filename.
        let odd = repo
            .path()
            .join(std::ffi::OsStr::from_bytes(b"caf\xe9.txt"));
        std::fs::write(&odd, "old\n").unwrap();
        let base = snapshot(cwd).await.unwrap();
        std::fs::write(&odd, "new\n").unwrap();
        std::fs::write(repo.path().join("plain.txt"), "plain\n").unwrap();
        let now = snapshot(cwd).await.unwrap();
        let files = changed_files(&now.root, &base.tree, &now.tree)
            .await
            .expect("a turn with an odd name still diffs");
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["plain.txt"]);
    }
}

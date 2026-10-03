//! Git owns checkout registrations; workspace closure never removes a checkout.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use signaltty_core::model::{GitInfo, LiveState, Workspace};
use signaltty_core::new_ws_id;
use signaltty_proto::{code, event};
use tokio::io::AsyncReadExt;

use crate::params::{
    bad_params, ParamError, WorkspaceId, WorktreeCreate, WorktreeOpen, WorktreeRemove,
};
use crate::router::{resolve_workspace, unique_handle, Ctx};

#[derive(Default)]
pub struct Worktrees {
    mutations: tokio::sync::Mutex<()>,
    pub references: PathReferences,
}

#[derive(Default)]
struct References {
    entering: HashMap<PathBuf, usize>,
    removing: Vec<PathBuf>,
}

#[derive(Default, Clone)]
pub struct PathReferences(Arc<Mutex<References>>);

pub struct PathReference {
    registry: PathReferences,
    path: PathBuf,
    removing: bool,
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

impl PathReferences {
    /// Hold until the new workspace/process is published into Store.
    pub fn enter(&self, cwd: &str) -> Result<PathReference, ParamError> {
        let path = canonical_directory(Path::new(cwd))?;
        let mut refs = self.0.lock().unwrap();
        if refs.removing.iter().any(|p| overlaps(p, &path)) {
            return Err(bad_params("checkout is being removed"));
        }
        *refs.entering.entry(path.clone()).or_default() += 1;
        Ok(PathReference {
            registry: self.clone(),
            path,
            removing: false,
        })
    }

    fn reserve_removal(&self, path: PathBuf) -> Result<PathReference, ParamError> {
        let mut refs = self.0.lock().unwrap();
        if refs.entering.keys().any(|p| overlaps(p, &path))
            || refs.removing.iter().any(|p| overlaps(p, &path))
        {
            return Err(bad_params(
                "checkout has a concurrent workspace or process operation",
            ));
        }
        refs.removing.push(path.clone());
        Ok(PathReference {
            registry: self.clone(),
            path,
            removing: true,
        })
    }
}

impl Drop for PathReference {
    fn drop(&mut self) {
        let mut refs = self.registry.0.lock().unwrap();
        if self.removing {
            refs.removing.retain(|p| p != &self.path);
        } else if let Some(count) = refs.entering.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                refs.entering.remove(&self.path);
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub main: bool,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
    pub workspace_id: Option<String>,
}

fn canonical_directory(path: &Path) -> Result<PathBuf, ParamError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| bad_params(format!("{}: {e}", path.display())))?;
    if !canonical.is_dir() {
        return Err(bad_params("path is not a directory"));
    }
    Ok(canonical)
}

fn absolute_target(path: &str, existing: bool) -> Result<PathBuf, ParamError> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(bad_params("worktree path must be absolute"));
    }
    if existing {
        return canonical_directory(path);
    }
    if path.exists() {
        return Err(bad_params("worktree path is already occupied"));
    }
    let parent = canonical_directory(
        path.parent()
            .ok_or_else(|| bad_params("invalid worktree path"))?,
    )?;
    let file = path
        .file_name()
        .ok_or_else(|| bad_params("invalid worktree path"))?;
    Ok(parent.join(file))
}

fn source(ctx: &Ctx, id: &str) -> Result<Workspace, ParamError> {
    let store = ctx.store.read().unwrap();
    let id = resolve_workspace(&store, id)
        .ok_or_else(|| (code::NO_SUCH_WORKSPACE.into(), id.to_owned()))?;
    Ok(store.workspaces[&id].clone())
}

struct GitProcessGroup(Option<nix::unistd::Pid>);

impl GitProcessGroup {
    fn kill(&mut self) {
        if let Some(pid) = self.0.take() {
            let _ = nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGKILL);
        }
    }
}

impl Drop for GitProcessGroup {
    fn drop(&mut self) {
        self.kill();
    }
}

async fn git(cwd: &str, args: &[&str]) -> Result<Vec<u8>, ParamError> {
    git_with_timeout(cwd, args, Duration::from_secs(20)).await
}

async fn git_with_timeout(
    cwd: &str,
    args: &[&str],
    deadline: Duration,
) -> Result<Vec<u8>, ParamError> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
    let mut group = GitProcessGroup(child.id().map(|pid| nix::unistd::Pid::from_raw(pid as i32)));
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let output = tokio::time::timeout(deadline, async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (status, _, _) = tokio::try_join!(
            child.wait(),
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err)
        )?;
        Ok::<_, std::io::Error>(std::process::Output {
            status,
            stdout: out,
            stderr: err,
        })
    })
    .await;
    let out = match output {
        Ok(Ok(out)) => out,
        failure => {
            group.kill();
            let _ = child.wait().await;
            return Err(match failure {
                Err(_) => (code::TIMEOUT.into(), "Git operation timed out".into()),
                Ok(Err(error)) => (code::IO_ERROR.into(), error.to_string()),
                Ok(Ok(_)) => unreachable!(),
            });
        }
    };
    if !out.status.success() {
        return Err(bad_params(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    Ok(out.stdout)
}

fn utf8(bytes: &[u8]) -> Result<String, ParamError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| bad_params("Git path is not valid UTF-8"))
}

async fn registered(cwd: &str) -> Result<Vec<Worktree>, ParamError> {
    let bytes = git(cwd, &["worktree", "list", "--porcelain", "-z"]).await?;
    let mut trees = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in bytes.split(|b| *b == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(tree) = current.take() {
                trees.push(tree);
            }
            current = Some(Worktree {
                path: utf8(path)?,
                branch: None,
                head: None,
                main: trees.is_empty(),
                bare: false,
                locked: false,
                prunable: false,
                workspace_id: None,
            });
        } else if let Some(tree) = current.as_mut() {
            if let Some(head) = field.strip_prefix(b"HEAD ") {
                tree.head = Some(utf8(head)?);
            } else if let Some(branch) = field.strip_prefix(b"branch refs/heads/") {
                tree.branch = Some(utf8(branch)?);
            } else if field == b"bare" {
                tree.bare = true;
            } else if field == b"locked" || field.starts_with(b"locked ") {
                tree.locked = true;
            } else if field == b"prunable" || field.starts_with(b"prunable ") {
                tree.prunable = true;
            }
        }
    }
    if let Some(tree) = current {
        trees.push(tree);
    }
    Ok(trees)
}

fn same_checkout(cwd: &str, target: &Path) -> bool {
    Path::new(cwd).canonicalize().is_ok_and(|p| p == target)
}

fn refers_to(cwd: &str, target: &Path) -> bool {
    Path::new(cwd)
        .canonicalize()
        .is_ok_and(|p| p.starts_with(target))
}

async fn managed_process_uses(ctx: &Ctx, target: PathBuf) -> Result<bool, ParamError> {
    let children = ctx.ptys.child_pids();
    let roots: HashSet<u32> = {
        let store = ctx.store.read().unwrap();
        children
            .into_iter()
            .filter_map(|(id, pid)| {
                store
                    .panes
                    .get(&id)
                    .filter(|pane| pane.live == LiveState::Live)
                    .map(|_| pid)
            })
            .collect()
    };
    if roots.is_empty() {
        return Ok(false);
    }
    tokio::task::spawn_blocking(move || -> Result<bool, ParamError> {
        let entries =
            std::fs::read_dir("/proc").map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
        let parents: HashMap<u32, u32> = entries
            .flatten()
            .filter_map(|entry| {
                let pid = entry.file_name().to_str()?.parse().ok()?;
                crate::attrib::parent_pid(pid).map(|parent| (pid, parent))
            })
            .collect();
        for pid in parents.keys() {
            let mut ancestor = *pid;
            let mut managed = roots.contains(&ancestor);
            for _ in 0..64 {
                if managed {
                    break;
                }
                let Some(parent) = parents.get(&ancestor).copied() else {
                    break;
                };
                if parent == ancestor {
                    break;
                }
                ancestor = parent;
                managed = roots.contains(&ancestor);
            }
            if managed
                && crate::procscan::proc_cwd(*pid).is_some_and(|cwd| refers_to(&cwd, &target))
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .await
    .map_err(|e| (code::IO_ERROR.into(), e.to_string()))?
}

fn membership(trees: Vec<Worktree>, target: &Path) -> Result<Worktree, ParamError> {
    trees
        .into_iter()
        .find(|w| same_checkout(&w.path, target))
        .ok_or_else(|| bad_params("path is not a registered worktree of this repository"))
}

pub async fn list(ctx: &Ctx, p: WorkspaceId) -> Result<Value, ParamError> {
    let ws = source(ctx, &p.workspace_id)?;
    let mut trees = registered(&ws.cwd).await?;
    let store = ctx.store.read().unwrap();
    for tree in &mut trees {
        if let Ok(path) = Path::new(&tree.path).canonicalize() {
            tree.workspace_id = store
                .workspaces
                .values()
                .find(|w| same_checkout(&w.cwd, &path))
                .map(|w| w.id.clone());
        }
    }
    Ok(json!({"worktrees": trees}))
}

async fn bind_workspace(
    ctx: &Ctx,
    source_id: &str,
    tree: Worktree,
    name: Option<String>,
) -> Result<Value, ParamError> {
    if name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        return Err(bad_params("workspace name must not be empty"));
    }
    let path = canonical_directory(Path::new(&tree.path))?;
    let cwd = utf8(path.as_os_str().as_encoded_bytes())?;
    let _reference = ctx.worktrees.references.enter(&cwd)?;
    let dirty = !git(
        &cwd,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    )
    .await?
    .is_empty();
    let now = Utc::now();
    let name = name.unwrap_or_else(|| {
        tree.branch.clone().unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
    });
    let mut store = ctx.store.write().unwrap();
    if !store.workspaces.contains_key(source_id) {
        return Err((
            code::NO_SUCH_WORKSPACE.into(),
            "source workspace closed".into(),
        ));
    }
    if let Some(ws) = store
        .workspaces
        .values()
        .find(|w| same_checkout(&w.cwd, &path))
    {
        return Ok(json!({"workspace":ws,"path":cwd,"reused":true}));
    }
    let workspace = Workspace {
        id: new_ws_id(),
        handle: unique_handle(&store, &signaltty_core::model::slugify(&name)),
        name,
        git: GitInfo {
            root: Some(cwd.clone()),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            branch: tree.branch.clone(),
            detached: tree.branch.is_none(),
            dirty: Some(dirty),
        },
        cwd: cwd.clone(),
        tabs: Vec::new(),
        active_tab_id: None,
        auto_resume: false,
        created_at: now,
        updated_at: now,
    };
    store
        .workspaces
        .insert(workspace.id.clone(), workspace.clone());
    store.emit(event::WORKSPACE_CREATED, json!({"workspace":workspace}));
    drop(store);

    ctx.mark_persist();
    Ok(json!({"workspace":workspace,"path":cwd,"reused":false}))
}

pub async fn create(ctx: &Ctx, p: WorktreeCreate) -> Result<Value, ParamError> {
    if p.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        return Err(bad_params("workspace name must not be empty"));
    }
    if p.branch.starts_with('-') || p.branch.trim().is_empty() {
        return Err(bad_params("invalid branch name"));
    }
    let _mutation = ctx.worktrees.mutations.lock().await;
    let ws = source(ctx, &p.workspace_id)?;
    let path = absolute_target(&p.path, false)?;
    let path = utf8(path.as_os_str().as_encoded_bytes())?;
    git(&ws.cwd, &["check-ref-format", "--branch", &p.branch]).await?;
    registered(&ws.cwd).await?;
    git(
        &ws.cwd,
        &["worktree", "add", "-b", &p.branch, "--", &path, "HEAD"],
    )
    .await.map_err(|(code, message)| {
        if code == signaltty_proto::code::TIMEOUT {
            (code, format!("{message}; checkout or branch may have been created; refresh worktree list and check {path} before retrying"))
        } else { (code, message) }
    })?;
    ctx.emit(
        event::WORKTREE_CHANGED,
        json!({"operation":"create","path":path,"workspace_id":ws.id}),
    );
    let result = async {
        let tree = membership(registered(&ws.cwd).await?, Path::new(&path))?;
        bind_workspace(ctx, &ws.id, tree, p.name).await
    }
    .await;
    result.map_err(|(code, msg)| (code, format!("{msg}; checkout retained at {path}")))
}

pub async fn open(ctx: &Ctx, p: WorktreeOpen) -> Result<Value, ParamError> {
    let _mutation = ctx.worktrees.mutations.lock().await;
    let ws = source(ctx, &p.workspace_id)?;
    let path = absolute_target(&p.path, true)?;
    let tree = membership(registered(&ws.cwd).await?, &path)?;
    if tree.bare || tree.prunable {
        return Err(bad_params("checkout is bare or prunable"));
    }
    let result = bind_workspace(ctx, &ws.id, tree, p.name).await?;
    if result["reused"] == false {
        ctx.emit(
            event::WORKTREE_CHANGED,
            json!({"operation":"open","path":result["path"],"workspace_id":ws.id}),
        );
    }
    Ok(result)
}

pub async fn remove(ctx: &Ctx, p: WorktreeRemove) -> Result<Value, ParamError> {
    let _mutation = ctx.worktrees.mutations.lock().await;
    let ws = source(ctx, &p.workspace_id)?;
    let path = absolute_target(&p.path, true)?;
    let tree = membership(registered(&ws.cwd).await?, &path)?;
    if tree.main || tree.bare || tree.locked || tree.prunable {
        return Err(bad_params(
            "cannot remove main, bare, locked or prunable worktree",
        ));
    }
    let _reservation = ctx.worktrees.references.reserve_removal(path.clone())?;
    if managed_process_uses(ctx, path.clone()).await? {
        return Err((
            code::PANES_ALIVE.into(),
            "worktree has a running pane".into(),
        ));
    }
    {
        let store = ctx.store.read().unwrap();
        if store
            .panes
            .values()
            .any(|pane| pane.live == LiveState::Live && refers_to(&pane.cwd, &path))
        {
            return Err((
                code::PANES_ALIVE.into(),
                "worktree has a running pane".into(),
            ));
        }
        if store.workspaces.values().any(|w| refers_to(&w.cwd, &path)) {
            return Err(bad_params(
                "close workspaces using this checkout before removing it",
            ));
        }
    }
    let cwd = utf8(path.as_os_str().as_encoded_bytes())?;
    if !git(
        &cwd,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .await?
    .is_empty()
    {
        return Err(bad_params("worktree has tracked or untracked changes"));
    }
    git(&ws.cwd, &["worktree", "remove", "--", &cwd]).await?;
    ctx.emit(
        event::WORKTREE_CHANGED,
        json!({"operation":"remove","path":cwd,"workspace_id":ws.id}),
    );
    Ok(json!({"removed":true,"path":cwd}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct HookRepository(PathBuf);

    impl HookRepository {
        async fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "signaltty-git-hook-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let cwd = root.to_str().unwrap();
            git(cwd, &["init", "-q"]).await.unwrap();
            git(cwd, &["config", "user.name", "test"]).await.unwrap();
            git(cwd, &["config", "user.email", "test@example.invalid"])
                .await
                .unwrap();
            git(cwd, &["config", "commit.gpgsign", "false"])
                .await
                .unwrap();
            std::fs::write(root.join("tracked.txt"), "base\n").unwrap();
            git(cwd, &["add", "."]).await.unwrap();
            git(cwd, &["commit", "-qm", "base"]).await.unwrap();
            let hook = root.join(".git/hooks/post-checkout");
            std::fs::write(&hook, "#!/bin/sh\nprintf started > hook-started\nsleep 1\nprintf late > hook-late-marker\n").unwrap();
            std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(root)
        }

        fn checkout(&self) -> PathBuf {
            self.0.join("checkout")
        }
    }

    impl Drop for HookRepository {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn timed_out_git_reaps_parent_and_stops_checkout_hook_descendants() {
        let repo = HookRepository::new().await;
        let checkout = repo.checkout();
        let error = git_with_timeout(
            repo.0.to_str().unwrap(),
            &[
                "worktree",
                "add",
                "-b",
                "timed-out",
                checkout.to_str().unwrap(),
            ],
            Duration::from_millis(500),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, code::TIMEOUT);
        assert!(
            checkout.join("hook-started").exists(),
            "test hook must start before timeout"
        );
        tokio::time::sleep(Duration::from_millis(750)).await;
        assert!(
            !checkout.join("hook-late-marker").exists(),
            "a timed-out Git hook kept mutating the checkout"
        );
    }

    #[tokio::test]
    async fn cancelled_git_stops_checkout_hook_descendants() {
        let repo = HookRepository::new().await;
        let cwd = repo.0.to_str().unwrap().to_owned();
        let checkout = repo.checkout();
        let path = checkout.to_str().unwrap().to_owned();
        let command =
            tokio::spawn(
                async move { git(&cwd, &["worktree", "add", "-b", "cancelled", &path]).await },
            );
        tokio::time::timeout(Duration::from_secs(3), async {
            while !checkout.join("hook-started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("test hook must start before cancellation");
        command.abort();
        assert!(command.await.unwrap_err().is_cancelled());
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(
            !checkout.join("hook-late-marker").exists(),
            "a cancelled Git hook kept mutating the checkout"
        );
    }
}

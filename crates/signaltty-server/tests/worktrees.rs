use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use signaltty_testkit::{TestClient, TestServer};

struct Repository(PathBuf);

impl Repository {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "signaltty-worktrees-{}-{}",
            std::process::id(),
            signaltty_core::ids::new_pane_id()
        ));
        std::fs::create_dir_all(path.join("main repo")).unwrap();
        let repo = Self(path);
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.main().join("tracked.txt"), "base\n").unwrap();
        repo.git(&["add", "."]);
        repo.git(&["commit", "-qm", "base"]);
        repo
    }

    fn main(&self) -> PathBuf {
        self.0.join("main repo")
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.main())
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {:?}", out.stderr);
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn worktree_checkout_isolated_reused_and_removed_without_branch_deletion() {
    let repo = Repository::new();
    let server = TestServer::start().await;
    let mut c = server.client().await;
    let source = c
        .call(
            "workspace.create",
            json!({"cwd":repo.main(),"name":"source"}),
        )
        .await
        .unwrap();
    let source_id = source["workspace"]["id"].clone();
    let source = source["workspace"]["handle"].as_str().unwrap();
    let mut subscriber = server.client().await;
    subscriber
        .call("subscribe", json!({"events":["worktree.changed"]}))
        .await
        .unwrap();
    let path = repo.0.join("feature checkout");
    let created = c
        .call(
            "worktree.create",
            json!({"workspace_id":source,"path":path,"branch":"feature","name":"feature"}),
        )
        .await
        .unwrap();
    assert_eq!(created["workspace"]["cwd"], path.to_str().unwrap());
    assert_eq!(created["workspace"]["git"]["branch"], "feature");
    assert!(path.join("tracked.txt").is_file());
    let opened = c
        .call("worktree.open", json!({"workspace_id":source,"path":path}))
        .await
        .unwrap();
    assert_eq!(created["workspace"]["id"], opened["workspace"]["id"]);
    assert_eq!(opened["reused"], true);
    let listed = c
        .call("worktree.list", json!({"workspace_id":source}))
        .await
        .unwrap();
    let entry = listed["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["path"] == path.to_str().unwrap())
        .unwrap();
    assert_eq!(entry["workspace_id"], created["workspace"]["id"]);
    assert_eq!(entry["main"], false);
    c.call(
        "workspace.close",
        json!({"workspace_id":created["workspace"]["id"]}),
    )
    .await
    .unwrap();
    assert!(
        path.is_dir(),
        "closing a workspace must preserve the checkout"
    );
    let removed = c
        .call(
            "worktree.remove",
            json!({"workspace_id":source,"path":path}),
        )
        .await
        .unwrap();
    assert_eq!(removed["removed"], true);
    assert!(!path.exists());
    repo.git(&["show-ref", "--verify", "refs/heads/feature"]);
    assert!(Path::new(&repo.main()).join("tracked.txt").exists());
    let events = subscriber
        .read_events(2, std::time::Duration::from_secs(3))
        .await;
    assert_eq!(events[0]["payload"]["operation"], "create");
    assert_eq!(events[0]["payload"]["workspace_id"], source_id);
    assert_eq!(events[1]["payload"]["operation"], "remove");
    assert_eq!(
        events[0]["payload"]["workspace_id"],
        events[1]["payload"]["workspace_id"]
    );
    assert_eq!(events[0]["payload"]["path"], path.to_str().unwrap());
    server.shutdown().await;
}

async fn refusal(c: &mut TestClient, method: &str, params: serde_json::Value, code: &str) {
    let error = c.call(method, params).await.unwrap_err();
    assert!(error.starts_with(code), "{error}");
}

#[tokio::test]
async fn worktree_removal_refuses_open_live_dirty_main_and_locked_checkouts() {
    let repo = Repository::new();
    let server = TestServer::start().await;
    let mut c = server.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd":repo.main()}))
        .await
        .unwrap();
    let source = ws["workspace"]["id"].as_str().unwrap();
    let path = repo.0.join("protected checkout");
    let created = c
        .call(
            "worktree.create",
            json!({"workspace_id":source,"path":path,"branch":"protected"}),
        )
        .await
        .unwrap();
    let target = created["workspace"]["id"].as_str().unwrap();
    let removal = json!({"workspace_id":source,"path":path});
    refusal(&mut c, "worktree.remove", removal.clone(), "BAD_PARAMS").await;
    let pane = c
        .call(
            "pane.spawn",
            json!({"workspace_id":target,"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    refusal(&mut c, "worktree.remove", removal.clone(), "PANES_ALIVE").await;
    c.call("pane.close", json!({"pane_id":pane["pane"]["id"]}))
        .await
        .unwrap();
    c.call("workspace.close", json!({"workspace_id":target}))
        .await
        .unwrap();
    std::fs::write(path.join("untracked.txt"), "do not delete\n").unwrap();
    refusal(&mut c, "worktree.remove", removal.clone(), "BAD_PARAMS").await;
    assert!(path.join("untracked.txt").exists());
    std::fs::remove_file(path.join("untracked.txt")).unwrap();
    std::fs::write(path.join("tracked.txt"), "modified\n").unwrap();
    refusal(&mut c, "worktree.remove", removal.clone(), "BAD_PARAMS").await;
    std::fs::write(path.join("tracked.txt"), "base\n").unwrap();
    repo.git(&["worktree", "lock", path.to_str().unwrap()]);
    refusal(&mut c, "worktree.remove", removal.clone(), "BAD_PARAMS").await;
    repo.git(&["worktree", "unlock", path.to_str().unwrap()]);
    refusal(
        &mut c,
        "worktree.remove",
        json!({"workspace_id":source,"path":repo.main()}),
        "BAD_PARAMS",
    )
    .await;
    c.call("worktree.remove", removal).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn worktree_open_accepts_external_detached_checkout_and_survives_restore() {
    let repo = Repository::new();
    let external = repo.0.join("external\tcheckout\n");
    repo.git(&[
        "worktree",
        "add",
        "--detach",
        external.to_str().unwrap(),
        "HEAD",
    ]);
    let mut server = TestServer::start().await;
    let mut c = server.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd":repo.main()}))
        .await
        .unwrap();
    let source = ws["workspace"]["id"].as_str().unwrap();
    let opened = c
        .call(
            "worktree.open",
            json!({"workspace_id":source,"path":external,"name":"detached"}),
        )
        .await
        .unwrap();
    assert_eq!(opened["workspace"]["git"]["detached"], true);
    server.restart().await;
    let mut c = server.client().await;
    let reopened = c
        .call(
            "worktree.open",
            json!({"workspace_id":source,"path":external}),
        )
        .await
        .unwrap();
    assert_eq!(opened["workspace"]["id"], reopened["workspace"]["id"]);
    assert_eq!(reopened["reused"], true);
    let listed = c
        .call("worktree.list", json!({"workspace_id":source}))
        .await
        .unwrap();
    assert!(listed["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tree| tree["path"] == external.to_str().unwrap() && tree["branch"].is_null()));
    server.shutdown().await;
}

#[tokio::test]
async fn worktree_inputs_are_typed_and_foreign_paths_and_duplicate_branches_are_rejected() {
    let repo = Repository::new();
    let foreign = Repository::new();
    let server = TestServer::start().await;
    let mut c = server.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd":repo.main()}))
        .await
        .unwrap();
    let source = ws["workspace"]["handle"].as_str().unwrap();
    refusal(
        &mut c,
        "worktree.list",
        json!({"workspace_id":42}),
        "BAD_PARAMS",
    )
    .await;
    refusal(
        &mut c,
        "worktree.list",
        json!({"workspace_id":"missing"}),
        "NO_SUCH_WORKSPACE",
    )
    .await;
    refusal(
        &mut c,
        "worktree.open",
        json!({"workspace_id":source,"path":foreign.main()}),
        "BAD_PARAMS",
    )
    .await;
    for (path, branch) in [
        ("relative", "feature"),
        (repo.0.to_str().unwrap(), "feature"),
        (repo.0.join("invalid").to_str().unwrap(), "--force"),
        (repo.0.join("invalid").to_str().unwrap(), "bad branch"),
    ] {
        refusal(
            &mut c,
            "worktree.create",
            json!({"workspace_id":source,"path":path,"branch":branch}),
            "BAD_PARAMS",
        )
        .await;
    }
    let path = repo.0.join("one");
    c.call(
        "worktree.create",
        json!({"workspace_id":source,"path":path,"branch":"duplicate","future_field":true}),
    )
    .await
    .unwrap();
    refusal(
        &mut c,
        "worktree.create",
        json!({"workspace_id":source,"path":repo.0.join("two"),"branch":"duplicate"}),
        "BAD_PARAMS",
    )
    .await;
    assert!(!repo.0.join("two").exists());
    server.shutdown().await;
}

#[tokio::test]
async fn removal_sees_a_background_process_directory_before_periodic_refresh() {
    let repo = Repository::new();
    let server = TestServer::start().await;
    let mut c = server.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd":repo.main()}))
        .await
        .unwrap();
    let source = ws["workspace"]["id"].as_str().unwrap();
    let path = repo.0.join("entered checkout");
    repo.git(&["worktree", "add", "-b", "entered", path.to_str().unwrap()]);
    let pane = c.call("pane.spawn", json!({"workspace_id":source,"argv":["sh","-c","sh -c 'cd \"$1\"; printf cwd-ready; sleep 30' sh \"$1\" & sleep 30; wait","sh",path]})).await.unwrap();
    let pane_id = pane["pane"]["id"].as_str().unwrap();
    let mut ready = false;
    for _ in 0..100 {
        let read = c
            .call(
                "pane.read",
                json!({"pane_id":pane_id,"mode":"tail","lines":5}),
            )
            .await
            .unwrap();
        if read["text"].as_str().unwrap().contains("cwd-ready") {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(ready, "background child did not enter the checkout");
    refusal(
        &mut c,
        "worktree.remove",
        json!({"workspace_id":source,"path":path}),
        "PANES_ALIVE",
    )
    .await;
    c.call("pane.close", json!({"pane_id":pane_id}))
        .await
        .unwrap();
    c.call(
        "worktree.remove",
        json!({"workspace_id":source,"path":path}),
    )
    .await
    .unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn removal_cannot_race_ordinary_workspace_creation_or_pane_launch() {
    let repo = Repository::new();
    let server = TestServer::start().await;
    let mut control = server.client().await;
    let ws = control
        .call("workspace.create", json!({"cwd":repo.main()}))
        .await
        .unwrap();
    let source = ws["workspace"]["id"].as_str().unwrap();
    for index in 0..6 {
        let path = repo.0.join(format!("racing-{index}"));
        repo.git(&[
            "worktree",
            "add",
            "-b",
            &format!("racing-{index}"),
            path.to_str().unwrap(),
        ]);
        let mut remover = server.client().await;
        let mut opener = server.client().await;
        let operation = if index % 2 == 0 {
            "workspace.create"
        } else {
            "pane.spawn"
        };
        let params = if index % 2 == 0 {
            json!({"cwd":path})
        } else {
            json!({"workspace_id":source,"cwd":path,"argv":["sleep","30"]})
        };
        let (removed, opened) = tokio::join!(
            remover.call(
                "worktree.remove",
                json!({"workspace_id":source,"path":path})
            ),
            opener.call(operation, params)
        );
        assert!(
            !(removed.is_ok() && opened.is_ok()),
            "removed a checkout while publishing a live reference"
        );
        if let Ok(opened) = opened {
            assert!(
                path.is_dir(),
                "published reference points to a removed checkout"
            );
            if index % 2 == 0 {
                control
                    .call(
                        "workspace.close",
                        json!({"workspace_id":opened["workspace"]["id"]}),
                    )
                    .await
                    .unwrap();
            } else {
                control
                    .call("pane.close", json!({"pane_id":opened["pane"]["id"]}))
                    .await
                    .unwrap();
            }
        }
        if path.exists() {
            control
                .call(
                    "worktree.remove",
                    json!({"workspace_id":source,"path":path}),
                )
                .await
                .unwrap();
        }
    }
    server.shutdown().await;
}

async fn cli(server: &TestServer, args: &[&str]) -> serde_json::Value {
    let output = tokio::process::Command::new(signaltty_testkit::bin_path("signaltty"))
        .arg("--socket")
        .arg(&server.socket)
        .arg("--json")
        .args(args)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI emits a single JSON result")
}

#[tokio::test]
async fn cli_worktree_commands_deliver_the_json_api_contract() {
    let repo = Repository::new();
    let server = TestServer::start().await;
    let mut c = server.client().await;
    let source = c
        .call(
            "workspace.create",
            json!({"cwd":repo.main(),"name":"CLI source"}),
        )
        .await
        .unwrap();
    let handle = source["workspace"]["handle"].as_str().unwrap();
    let path = repo.0.join("cli checkout");
    let path = path.to_str().unwrap();
    let created = cli(
        &server,
        &[
            "worktree",
            "create",
            "--workspace",
            handle,
            "--path",
            path,
            "--branch",
            "cli-feature",
            "--name",
            "CLI feature",
        ],
    )
    .await;
    assert_eq!(created["workspace"]["git"]["branch"], "cli-feature");
    let listed = cli(&server, &["worktree", "list", "--workspace", handle]).await;
    assert_eq!(listed["worktrees"].as_array().unwrap().len(), 2);
    let opened = cli(
        &server,
        &["worktree", "open", "--workspace", handle, "--path", path],
    )
    .await;
    assert_eq!(opened["workspace"]["id"], created["workspace"]["id"]);
    c.call(
        "workspace.close",
        json!({"workspace_id":created["workspace"]["id"]}),
    )
    .await
    .unwrap();
    let removed = cli(
        &server,
        &["worktree", "remove", "--workspace", handle, "--path", path],
    )
    .await;
    assert_eq!(removed["removed"], true);
    assert!(!Path::new(path).exists());
    server.shutdown().await;
}

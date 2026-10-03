use std::path::PathBuf;
use std::process::Command;

use serde_json::{json, Value};
use signaltty_testkit::{TestClient, TestServer};

struct Repository(PathBuf);

impl Repository {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "signaltty-file-diff-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Self(dir);
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {:?}", output.stderr);
    }

    fn write(&self, path: &str, text: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(path), text).unwrap();
    }

    async fn workspace(&self, client: &mut TestClient) -> Value {
        client
            .call("workspace.create", json!({"cwd":self.0,"name":"diff"}))
            .await
            .unwrap()["workspace"]
            .clone()
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn read(client: &mut TestClient, workspace: &Value, path: &str) -> Value {
    client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":workspace["handle"],"path":path}),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn tracked_diff_combines_staged_and_unstaged_and_reads_deletion() {
    let repo = Repository::new();
    repo.write(
        "tracked.txt",
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve\n",
    );
    repo.write("deleted.txt", "gone\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    repo.write(
        "tracked.txt",
        "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve\n",
    );
    repo.git(&["add", "tracked.txt"]);
    repo.write(
        "tracked.txt",
        "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\nTWELVE\n",
    );
    std::fs::remove_file(repo.0.join("deleted.txt")).unwrap();
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    let diff = read(&mut client, &workspace, "tracked.txt").await;
    assert_eq!(diff["workspace_id"], workspace["id"]);
    assert_eq!(diff["content"]["kind"], "text");
    assert_eq!(diff["content"]["truncated"], false);
    let hunks = diff["content"]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 2);
    assert_eq!(
        hunks[0]["lines"][2],
        json!({"kind":"context","text":"two","old_line":2,"new_line":2})
    );
    assert_eq!(
        hunks[0]["lines"][0],
        json!({"kind":"removed","text":"one","old_line":1,"new_line":null})
    );
    assert_eq!(
        hunks[0]["lines"][1],
        json!({"kind":"added","text":"ONE","old_line":null,"new_line":1})
    );
    assert_eq!(
        hunks[1]["lines"].as_array().unwrap().last().unwrap()["text"],
        "TWELVE"
    );
    let deleted = read(&mut client, &workspace, "deleted.txt").await;
    assert_eq!(deleted["content"]["hunks"][0]["lines"][0]["text"], "gone");
    let original = client
        .call("workspace.get", json!({"workspace_id":workspace["id"]}))
        .await
        .unwrap();
    let sequence = client
        .call("subscribe", json!({"events":["workspace.*","git.*"]}))
        .await
        .unwrap()["seq"]
        .clone();
    read(&mut client, &workspace, "tracked.txt").await;
    assert_eq!(
        client
            .call("workspace.get", json!({"workspace_id":workspace["id"]}))
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        client
            .call("subscribe", json!({"events":["workspace.*","git.*"]}))
            .await
            .unwrap()["seq"],
        sequence
    );
    server.shutdown().await;
}

#[tokio::test]
async fn git_configuration_cannot_change_patch_syntax_or_run_diff_tools() {
    let repo = Repository::new();
    repo.write("text.txt", "first\n\nlast\n");
    repo.write(".gitattributes", "text.txt diff=custom\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    repo.git(&["config", "diff.suppressBlankEmpty", "true"]);
    repo.git(&["config", "diff.outputIndicatorNew", "A"]);
    repo.git(&["config", "diff.outputIndicatorOld", "R"]);
    repo.git(&["config", "diff.outputIndicatorContext", "C"]);
    repo.git(&["config", "diff.custom.command", "touch invoked-diff"]);
    repo.git(&["config", "diff.custom.textconv", "touch invoked-textconv"]);
    repo.git(&["config", "color.ui", "always"]);
    repo.write("text.txt", "FIRST\n\nlast\n");
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    let diff = read(&mut client, &workspace, "text.txt").await;
    assert_eq!(diff["content"]["kind"], "text", "{diff}");
    assert_eq!(diff["content"]["hunks"][0]["lines"][0]["kind"], "removed");
    assert_eq!(diff["content"]["hunks"][0]["lines"][2]["text"], "");
    assert!(!repo.0.join("invoked-diff").exists());
    assert!(!repo.0.join("invoked-textconv").exists());
    server.shutdown().await;
}

#[tokio::test]
async fn new_binary_empty_and_unborn_files_have_truthful_content() {
    let repo = Repository::new();
    repo.write("staged.txt", "first\nsecond");
    repo.git(&["add", "staged.txt"]);
    repo.write("new.txt", "new\nlast");
    repo.write("empty.txt", "");
    repo.write("binary.dat", [0, 1, 2]);
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    let staged = read(&mut client, &workspace, "staged.txt").await;
    assert_eq!(staged["untracked"], false);
    assert_eq!(staged["content"]["hunks"][0]["old_count"], 0);
    let new = read(&mut client, &workspace, "new.txt").await;
    assert_eq!(new["untracked"], true);
    assert_eq!(new["content"]["hunks"][0]["lines"][1]["text"], "last");
    assert_eq!(new["content"]["hunks"][0]["lines"][2]["kind"], "no_newline");
    let empty = read(&mut client, &workspace, "empty.txt").await;
    assert_eq!(empty["content"]["hunks"], json!([]));
    assert!(empty["content"]["notice"]
        .as_str()
        .unwrap()
        .contains("Empty"));
    assert_eq!(
        read(&mut client, &workspace, "binary.dat").await["content"]["kind"],
        "binary"
    );
    let summary = client
        .call("workspace.diff", json!({"workspace_id":workspace["id"]}))
        .await
        .unwrap();
    assert_eq!(summary["added"], 2);
    server.shutdown().await;
}

#[tokio::test]
async fn literal_paths_and_subdirectory_workspaces_share_root_relative_names() {
    let repo = Repository::new();
    std::fs::create_dir(repo.0.join("sub")).unwrap();
    let names = [
        "root.txt",
        "sub/inside.txt",
        "<markup>&[x]\tline\n.txt",
        "-option",
        ":(glob)*.txt",
        "star*.txt",
    ];
    for name in names {
        repo.write(name, "old\n");
    }
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    for name in names {
        repo.write(name, "new\n");
    }
    repo.write("root-new.txt", "ROOT NEW\n");
    repo.write("sub/sub-new.txt", "SUB NEW\n");
    repo.write("other.txt", "SHOULD NOT MATCH\n");
    repo.git(&["config", "diff.relative", "true"]);
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = client
        .call(
            "workspace.create",
            json!({"cwd":repo.0.join("sub"),"name":"sub"}),
        )
        .await
        .unwrap()["workspace"]
        .clone();
    let summary = client
        .call("workspace.diff", json!({"workspace_id":workspace["id"]}))
        .await
        .unwrap();
    for name in names.into_iter().chain(["root-new.txt", "sub/sub-new.txt"]) {
        assert!(
            summary["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["path"] == name),
            "missing {name:?}: {summary}"
        );
        let diff = read(&mut client, &workspace, name).await;
        assert_eq!(diff["path"], name);
        assert_eq!(diff["content"]["kind"], "text", "{diff}");
        assert!(diff["content"]["hunks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|h| h["lines"].as_array().unwrap())
            .all(|l| l["text"] != "SHOULD NOT MATCH"));
    }
    let error = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":workspace["id"],"path":"sub"}),
        )
        .await
        .unwrap_err();
    assert!(
        error.starts_with("BAD_PARAMS"),
        "directory must not read subtree: {error}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn tracked_diff_preserves_git_normalization_binary_attributes_and_clean_state() {
    let repo = Repository::new();
    repo.write(
        ".gitattributes",
        "text.txt text eol=crlf\nforced.dat -diff\n",
    );
    repo.write("text.txt", "old\r\n");
    repo.write("forced.dat", "text classified as binary\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    repo.write("forced.dat", "new text still binary\n");
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    assert_eq!(
        read(&mut client, &workspace, "text.txt").await["content"]["kind"],
        "unchanged"
    );
    assert_eq!(
        read(&mut client, &workspace, "forced.dat").await["content"]["kind"],
        "binary"
    );
    repo.write("text.txt", "new\r\n");
    let diff = read(&mut client, &workspace, "text.txt").await;
    assert_eq!(diff["content"]["hunks"][0]["lines"][1]["text"], "new");
    server.shutdown().await;
}

#[tokio::test]
async fn typed_path_validation_and_nonmembers_are_rejected() {
    let repo = Repository::new();
    repo.write(".gitignore", "ignored.txt\n");
    repo.write("ignored.txt", "ignored\n");
    repo.write("ok.txt", "okay\n");
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    for path in [
        "",
        "/etc/passwd",
        "../outside",
        "./ok.txt",
        "sub/../ok.txt",
        ".git/config",
        "sub/.git/config",
        "missing.txt",
        "ignored.txt",
        "nul\0.txt",
    ] {
        let error = client
            .call(
                "workspace.file_diff",
                json!({"workspace_id":workspace["id"],"path":path}),
            )
            .await
            .unwrap_err();
        assert!(error.starts_with("BAD_PARAMS"), "{path:?}: {error}");
    }
    for params in [
        json!({"workspace_id":workspace["id"]}),
        json!({"workspace_id":workspace["id"],"path":4}),
        json!({"workspace_id":4,"path":"ok.txt"}),
    ] {
        assert!(client
            .call("workspace.file_diff", params)
            .await
            .unwrap_err()
            .starts_with("BAD_PARAMS"));
    }
    assert!(client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":"ws_missing","path":"ok.txt"})
        )
        .await
        .unwrap_err()
        .starts_with("NO_SUCH_WORKSPACE"));
    assert_eq!(
        client
            .call(
                "workspace.file_diff",
                json!({"workspace_id":workspace["id"],"path":"ok.txt","future":true})
            )
            .await
            .unwrap()["content"]["kind"],
        "text"
    );
    std::fs::create_dir_all(&server.integration_home).unwrap();
    let nonrepo = client
        .call("workspace.create", json!({"cwd":server.integration_home}))
        .await
        .unwrap();
    assert!(client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":nonrepo["workspace"]["id"],"path":"ok.txt"})
        )
        .await
        .unwrap_err()
        .starts_with("BAD_PARAMS"));
    let schema = client.call("server.schema", json!({})).await.unwrap();
    assert!(schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "workspace.file_diff"));
    server.shutdown().await;
}

#[tokio::test]
async fn unreadable_untracked_regular_file_reports_io_error() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repository::new();
    repo.write("private.txt", "private content\n");
    std::fs::set_permissions(
        repo.0.join("private.txt"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    let error = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":workspace["id"],"path":"private.txt"}),
        )
        .await
        .unwrap_err();
    assert!(error.starts_with("IO_ERROR"), "{error}");
    server.shutdown().await;
}

#[tokio::test]
async fn symlinks_and_special_files_never_preview_external_contents() {
    use std::os::unix::fs::symlink;
    let repo = Repository::new();
    let outside = Repository::new();
    outside.write("child.txt", "EXTERNAL SECRET\n");
    std::fs::create_dir(repo.0.join("dir")).unwrap();
    repo.write("dir/child.txt", "repository text\n");
    repo.write("leaf.txt", "leaf repository text\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    std::fs::remove_dir_all(repo.0.join("dir")).unwrap();
    symlink(&outside.0, repo.0.join("dir")).unwrap();
    std::fs::remove_file(repo.0.join("leaf.txt")).unwrap();
    symlink(outside.0.join("child.txt"), repo.0.join("leaf.txt")).unwrap();
    symlink(outside.0.join("child.txt"), repo.0.join("new-link.txt")).unwrap();
    let fifo = Command::new("mkfifo")
        .arg(repo.0.join("fifo"))
        .status()
        .unwrap();
    assert!(fifo.success());
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    let parent = read(&mut client, &workspace, "dir/child.txt").await;
    assert_eq!(
        parent["content"]["hunks"][0]["lines"][0]["text"],
        "repository text"
    );
    let leaf = read(&mut client, &workspace, "leaf.txt").await;
    assert_eq!(leaf["content"]["kind"], "text", "{leaf}");
    assert_eq!(leaf["content"]["hunks"].as_array().unwrap().len(), 2);
    assert!(!leaf.to_string().contains("EXTERNAL SECRET"));
    assert_eq!(
        read(&mut client, &workspace, "new-link.txt").await["content"]["kind"],
        "unavailable"
    );
    let started = std::time::Instant::now();
    let result = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id":workspace["id"],"path":"fifo"}),
        )
        .await;
    match result {
        Ok(diff) => assert_eq!(diff["content"]["kind"], "unavailable"),
        Err(error) => assert!(error.starts_with("BAD_PARAMS"), "{error}"),
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    server.shutdown().await;
}

#[tokio::test]
async fn preview_byte_and_line_limits_are_explicit_and_keep_complete_lines() {
    let repo = Repository::new();
    repo.write("tracked.txt", "old\n");
    repo.write("tracked-many.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    repo.write("tracked.txt", "x".repeat(512 * 1024 + 1));
    repo.write("big-new.txt", "x".repeat(512 * 1024 + 1));
    repo.write("many.txt", "line\n".repeat(10_001));
    repo.write("tracked-many.txt", "line\n".repeat(10_001));
    repo.write("non-utf8.txt", [0xff, 0xfe]);
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    for path in ["tracked.txt", "big-new.txt"] {
        let diff = read(&mut client, &workspace, path).await;
        assert_eq!(diff["content"]["kind"], "unavailable", "{diff}");
        assert!(diff["content"]["reason"]
            .as_str()
            .unwrap()
            .contains("512 KiB"));
    }
    let many = read(&mut client, &workspace, "many.txt").await;
    assert_eq!(many["content"]["truncated"], true);
    assert_eq!(
        many["content"]["hunks"][0]["lines"]
            .as_array()
            .unwrap()
            .len(),
        10_000
    );
    assert_eq!(many["content"]["hunks"][0]["lines"][9999]["text"], "line");
    assert!(many["content"]["notice"]
        .as_str()
        .unwrap()
        .contains("truncated"));
    let tracked_many = read(&mut client, &workspace, "tracked-many.txt").await;
    assert_eq!(tracked_many["content"]["truncated"], true);
    assert_eq!(
        tracked_many["content"]["hunks"][0]["lines"]
            .as_array()
            .unwrap()
            .len(),
        10_000
    );
    assert_eq!(
        read(&mut client, &workspace, "non-utf8.txt").await["content"]["kind"],
        "unavailable"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn git_deadline_kills_filter_descendants_and_other_control_calls_stay_responsive() {
    let repo = Repository::new();
    repo.write("tracked.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-qm", "base"]);
    let server = TestServer::start().await;
    std::fs::create_dir_all(&server.integration_home).unwrap();
    let mut client = server.client().await;
    let workspace = repo.workspace(&mut client).await;
    repo.write(
        "slow.sh",
        "echo $$ > \"$SIGNALTTY_INTEGRATION_HOME/filter.pid\"\nsleep 30 &\necho $! > \"$SIGNALTTY_INTEGRATION_HOME/sleeper.pid\"\nwait\ncat\n",
    );
    repo.write(".gitattributes", "tracked.txt filter=slow\n");
    repo.git(&["config", "filter.slow.clean", "sh slow.sh"]);
    repo.write("tracked.txt", "modified\n");
    let workspace_id = workspace["id"].clone();
    let started = std::time::Instant::now();
    let request = tokio::spawn(async move {
        client
            .call(
                "workspace.file_diff",
                json!({"workspace_id":workspace_id,"path":"tracked.txt"}),
            )
            .await
    });
    for _ in 0..100 {
        if server.integration_home.join("sleeper.pid").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        server.integration_home.join("sleeper.pid").exists(),
        "clean filter should run inside bounded Git request"
    );
    let mut control = server.client().await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        control.call("server.status", json!({})),
    )
    .await
    .unwrap()
    .unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(10), request)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.starts_with("TIMEOUT"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    for name in ["filter.pid", "sleeper.pid"] {
        let pid = std::fs::read_to_string(server.integration_home.join(name)).unwrap();
        for _ in 0..50 {
            let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
            if stat.as_ref().is_err()
                || stat.as_ref().is_ok_and(|s| {
                    s.split(')')
                        .nth(1)
                        .is_some_and(|s| s.trim_start().starts_with('Z'))
                })
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim())) {
            assert!(
                stat.split(')')
                    .nth(1)
                    .unwrap()
                    .trim_start()
                    .starts_with('Z'),
                "filter descendant must not remain running: {stat}"
            );
        }
    }
    server.shutdown().await;
}

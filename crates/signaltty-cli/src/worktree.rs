use clap::Subcommand;
use serde_json::json;

use crate::client::{CliError, Client};

#[derive(Debug, Subcommand)]
pub enum WorktreeOp {
    /// List registered checkouts and their open workspaces.
    List {
        #[arg(long)]
        workspace: String,
    },
    /// Create a checkout on a new branch and open it as a workspace.
    Create {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Open an existing registered checkout, reusing its workspace.
    Open {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a clean, unused checkout; its branch is kept.
    Remove {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        path: String,
    },
}

pub async fn run(client: &mut Client, op: WorktreeOp, json_mode: bool) -> Result<(), CliError> {
    let (method, params) = match op {
        WorktreeOp::List { workspace } => ("worktree.list", json!({"workspace_id":workspace})),
        WorktreeOp::Create {
            workspace,
            path,
            branch,
            name,
        } => (
            "worktree.create",
            json!({"workspace_id":workspace,"path":path,"branch":branch,"name":name}),
        ),
        WorktreeOp::Open {
            workspace,
            path,
            name,
        } => (
            "worktree.open",
            json!({"workspace_id":workspace,"path":path,"name":name}),
        ),
        WorktreeOp::Remove { workspace, path } => (
            "worktree.remove",
            json!({"workspace_id":workspace,"path":path}),
        ),
    };
    let result = client.call(method, params).await?;
    let human = if let Some(trees) = result["worktrees"].as_array() {
        trees
            .iter()
            .map(|tree| {
                format!(
                    "{}  {}{}{}",
                    tree["path"].as_str().unwrap_or("?"),
                    tree["branch"].as_str().unwrap_or("(detached)"),
                    if tree["main"] == true { " [main]" } else { "" },
                    tree["workspace_id"]
                        .as_str()
                        .map(|id| format!("  {id}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else if result["removed"] == true {
        format!("removed {}", result["path"].as_str().unwrap_or("?"))
    } else {
        format!(
            "workspace {} ({}) at {}",
            result["workspace"]["id"].as_str().unwrap_or("?"),
            result["workspace"]["handle"].as_str().unwrap_or("?"),
            result["path"].as_str().unwrap_or("?")
        )
    };
    crate::emit(json_mode, &result, human);
    Ok(())
}

//! Reporting adapter for the shared hook installer (ADR-0013).
use crate::client::CliError;
use signaltty_integration::{Hooks, Report};
use std::path::{Path, PathBuf};

pub fn home_dir(override_home: Option<&str>) -> Result<PathBuf, CliError> {
    override_home
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .ok_or_else(|| CliError::Usage("HOME is not set (use --home)".into()))
}
pub fn valid_agents() -> &'static [&'static str] {
    signaltty_integration::AGENTS
}
fn hooks(home: Option<&Path>) -> Result<Hooks, CliError> {
    Hooks::from_env(
        home,
        std::env::current_exe().map_err(|e| CliError::Io(e.to_string()))?,
    )
    .map_err(|e| CliError::Usage(e.to_string()))
}
fn print(report: Report, removing: bool, json: bool) {
    if json {
        println!("{}", serde_json::to_value(report).unwrap());
    } else {
        println!(
            "{} {} hooks ({})",
            if removing {
                "removed"
            } else if report.changed {
                "configured"
            } else {
                "already configured"
            },
            report.agent,
            report.file.display()
        );
        if let Some(notice) = report.notice {
            eprintln!("{notice}");
        }
    }
}
pub fn install(home: Option<&Path>, agent: &str, json: bool) -> Result<(), CliError> {
    print(
        hooks(home)?
            .install(agent)
            .map_err(|e| CliError::Usage(e.to_string()))?,
        false,
        json,
    );
    Ok(())
}
pub fn uninstall(home: Option<&Path>, agent: &str, json: bool) -> Result<(), CliError> {
    print(
        hooks(home)?
            .uninstall(agent)
            .map_err(|e| CliError::Usage(e.to_string()))?,
        true,
        json,
    );
    Ok(())
}
pub fn status(home: Option<&Path>, agents: &[String], json: bool) -> Result<(), CliError> {
    let list: Vec<&str> = if agents.is_empty() {
        valid_agents().to_vec()
    } else {
        agents.iter().map(String::as_str).collect()
    };
    let hooks = hooks(home)?;
    let mut out = serde_json::json!({});
    for agent in &list {
        let path = hooks
            .file_for(agent)
            .map_err(|e| CliError::Usage(e.to_string()))?;
        let installed = hooks
            .installed(agent)
            .map_err(|e| CliError::Usage(e.to_string()))?;
        out[agent] = serde_json::json!({"installed": installed, "file": path,
            "trust": if *agent == "codex" { "provider_review" } else { "provider_managed" }});
    }
    // Detection overlays (data, not code): manifests in the agents dir.
    let agents_dir = std::env::var("SIGNALTTY_AGENTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| signaltty_core::paths::agents_dir());
    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        files.sort();
        for path in files {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let entry = match std::fs::read_to_string(&path) {
                Ok(text) => match signaltty_agent::parse_manifest(&text) {
                    Ok(m) => serde_json::json!({
                        "file": path.display().to_string(),
                        "kind": m.kind().map(|k| k.as_str().to_string()).unwrap_or_default(),
                        "ok": true,
                    }),
                    Err(e) => serde_json::json!({
                        "file": path.display().to_string(),
                        "ok": false, "error": e,
                    }),
                },
                Err(e) => serde_json::json!({
                    "file": path.display().to_string(),
                    "ok": false, "error": e.to_string(),
                }),
            };
            manifests.push(serde_json::json!({"name": name, "manifest": entry}));
        }
    }
    out["manifests"] = serde_json::Value::Array(manifests);
    if json {
        println!("{out}");
    } else {
        for agent in &list {
            let e = &out[agent];
            println!(
                "{agent}: {} ({})",
                if e["installed"].as_bool().unwrap() {
                    "installed"
                } else {
                    "not installed"
                },
                e["file"].as_str().unwrap()
            );
        }
        let manifests = out["manifests"].as_array().cloned().unwrap_or_default();
        if manifests.is_empty() {
            println!("manifests: none ({})", agents_dir.display());
        }
        for m in &manifests {
            let inner = &m["manifest"];
            if inner["ok"].as_bool().unwrap_or(false) {
                println!(
                    "manifest {}: kind={} ({})",
                    m["name"].as_str().unwrap_or("?"),
                    inner["kind"].as_str().unwrap_or("?"),
                    inner["file"].as_str().unwrap_or("?"),
                );
            } else {
                println!(
                    "manifest {}: BROKEN: {} ({})",
                    m["name"].as_str().unwrap_or("?"),
                    inner["error"].as_str().unwrap_or("?"),
                    inner["file"].as_str().unwrap_or("?"),
                );
            }
        }
    }
    Ok(())
}

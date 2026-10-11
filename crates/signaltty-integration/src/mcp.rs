//! Managed stdio registrations; no socket or pane values are persisted.
use super::{atomic_write, lock, parse, read, Error, Hooks};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, Array, DocumentMut, Item, Table};

const NAME: &str = "signaltty";
const ARGS: &[&str] = &["mcp", "serve", "--managed-registration"];
const ENV: &[&str] = &["SIGNALTTY_SOCKET", "SIGNALTTY_PANE"];

#[derive(Debug, Serialize)]
pub struct Report {
    pub file: PathBuf,
    pub changed: bool,
    pub disabled: bool,
}

fn executable(command: &str) -> bool {
    Path::new(command).is_absolute()
}

impl Hooks {
    pub fn mcp_file_for(&self, agent: &str) -> Result<PathBuf, Error> {
        Ok(match agent {
            "codex" => self.codex_home.join("config.toml"),
            "claude" => self.claude_mcp.clone(),
            "cursor" => self.home.join(".cursor/mcp.json"),
            "opencode" => self.opencode_home.join("opencode.json"),
            _ => return Err(Error(format!("unknown agent '{agent}'"))),
        })
    }

    pub fn mcp_installed(&self, agent: &str) -> Result<bool, Error> {
        let path = self.mcp_file_for(agent)?;
        let Some(text) = read(&path)? else {
            return Ok(false);
        };
        if agent == "codex" {
            let doc = text
                .parse::<DocumentMut>()
                .map_err(|e| Error(format!("{}: invalid TOML: {e}", path.display())))?;
            Ok(doc
                .get("mcp_servers")
                .and_then(|s| s.get(NAME))
                .is_some_and(codex_owned))
        } else {
            let root = parse(&path, &text)?;
            Ok(json_owned(&root[key(agent)][NAME], agent))
        }
    }

    pub(super) fn update_mcp(&self, agent: &str, remove: bool) -> Result<Report, Error> {
        let destination = self.mcp_file_for(agent)?;
        if remove && !destination.try_exists()? {
            return Ok(Report {
                file: destination,
                changed: false,
                disabled: false,
            });
        }
        let path = match fs::symlink_metadata(&destination) {
            Ok(m) if m.file_type().is_symlink() => fs::canonicalize(&destination)?,
            _ => destination.clone(),
        };
        let parent = path
            .parent()
            .ok_or_else(|| Error("configuration has no parent".into()))?;
        fs::create_dir_all(parent)?;
        let lock_path = path.with_file_name(format!(
            ".{}.signaltty.lock",
            path.file_name().unwrap().to_string_lossy()
        ));
        let _lock = lock(&lock_path)?;
        let before = read(&path)?;
        let (after, disabled) = if agent == "codex" {
            codex(before.as_deref().unwrap_or(""), &self.reporter, remove)?
        } else {
            let mut root = match &before {
                Some(text) => parse(&path, text)?,
                None => json!({}),
            };
            let original = root.clone();
            let disabled = merge_json(&mut root, agent, &self.reporter, remove)?;
            (
                if root == original {
                    before.clone().unwrap_or_default()
                } else {
                    serde_json::to_string_pretty(&root).unwrap() + "\n"
                },
                disabled,
            )
        };
        let changed = before.as_deref().unwrap_or("") != after;
        if changed {
            if read(&path)? != before {
                return Err(Error(format!(
                    "{} changed during setup; retry",
                    destination.display()
                )));
            }
            atomic_write(&path, after.as_bytes())?;
        }
        Ok(Report {
            file: destination,
            changed,
            disabled,
        })
    }
}

fn key(agent: &str) -> &'static str {
    if agent == "opencode" {
        "mcp"
    } else {
        "mcpServers"
    }
}

fn json_owned(entry: &Value, agent: &str) -> bool {
    if agent == "opencode" {
        entry["command"].as_array().is_some_and(|cmd| {
            cmd.len() == ARGS.len() + 1
                && cmd[0].as_str().is_some_and(executable)
                && cmd[1..] == json!(ARGS).as_array().unwrap()[..]
        })
    } else {
        entry["command"].as_str().is_some_and(executable) && entry["args"] == json!(ARGS)
    }
}

fn merge_json(root: &mut Value, agent: &str, reporter: &Path, remove: bool) -> Result<bool, Error> {
    if remove && root.get(key(agent)).is_none() {
        return Ok(false);
    }
    let servers = root
        .as_object_mut()
        .unwrap()
        .entry(key(agent))
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| Error(format!("{} must be an object", key(agent))))?;
    if let Some(entry) = servers.get(NAME) {
        if !json_owned(entry, agent) {
            if remove {
                return Ok(false);
            }
            return Err(Error(
                "MCP entry 'signaltty' belongs to another configuration; it was not replaced"
                    .into(),
            ));
        }
        if remove {
            servers.remove(NAME);
            return Ok(false);
        }
        if entry["disabled"] == true || entry["enabled"] == false {
            return Ok(true);
        }
    } else if remove {
        return Ok(false);
    }
    let entry = servers.entry(NAME).or_insert_with(|| json!({}));
    if agent == "opencode" {
        entry["type"] = json!("local");
        entry["command"] = json!([reporter, "mcp", "serve", "--managed-registration"]);
    } else {
        entry["command"] = json!(reporter);
        entry["args"] = json!(ARGS);
        entry["type"] = json!("stdio");
    }
    Ok(false)
}

fn codex_owned(entry: &Item) -> bool {
    entry
        .get("command")
        .and_then(Item::as_str)
        .is_some_and(executable)
        && entry
            .get("args")
            .and_then(Item::as_array)
            .is_some_and(|args| {
                args.len() == ARGS.len()
                    && args.iter().zip(ARGS).all(|(a, b)| a.as_str() == Some(b))
            })
}

fn codex(text: &str, reporter: &Path, remove: bool) -> Result<(String, bool), Error> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| Error(format!("invalid Codex TOML: {e}")))?;
    if remove && doc.get("mcp_servers").is_none() {
        return Ok((text.into(), false));
    }
    if doc.get("mcp_servers").is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        doc["mcp_servers"] = Item::Table(table);
    }
    let servers = doc["mcp_servers"]
        .as_table_like_mut()
        .ok_or_else(|| Error("mcp_servers must be a table".into()))?;
    if let Some(entry) = servers.get(NAME) {
        if !codex_owned(entry) {
            if remove {
                return Ok((text.into(), false));
            }
            return Err(Error(
                "MCP entry 'signaltty' belongs to another configuration; it was not replaced"
                    .into(),
            ));
        }
        if remove {
            servers.remove(NAME);
            return Ok((doc.to_string(), false));
        }
        if entry.get("enabled").and_then(Item::as_bool) == Some(false) {
            return Ok((text.into(), true));
        }
    } else if remove {
        return Ok((text.into(), false));
    }
    if servers.get(NAME).is_none() {
        servers.insert(NAME, Item::Table(Table::new()));
    }
    let entry = servers
        .get_mut(NAME)
        .unwrap()
        .as_table_like_mut()
        .ok_or_else(|| Error("signaltty MCP must be a table".into()))?;
    entry.insert("command", value(reporter.to_string_lossy().as_ref()));
    entry.insert("args", value(ARGS.iter().copied().collect::<Array>()));
    if entry.get("env_vars").is_none() {
        entry.insert("env_vars", value(Array::new()));
    }
    let env = entry
        .get_mut("env_vars")
        .unwrap()
        .as_array_mut()
        .ok_or_else(|| Error("signaltty MCP env_vars must be an array".into()))?;
    for name in ENV {
        if !env.iter().any(|v| v.as_str() == Some(name)) {
            env.push(*name);
        }
    }
    Ok((doc.to_string(), false))
}

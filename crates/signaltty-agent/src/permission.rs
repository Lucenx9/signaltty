//! Narrow native PermissionRequest contract shared by Claude and Codex.
use serde_json::{json, Value};
use signaltty_core::model::DecisionOption;

pub struct NativePermission {
    pub session_id: String,
    pub prompt: String,
}

pub fn native_permission(agent: &str, payload: &Value) -> Result<NativePermission, String> {
    if !matches!(agent, "claude" | "codex") {
        return Err("agent has no native permission channel".into());
    }
    let string = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("PermissionRequest needs a nonempty {key}"))
    };
    let session_id = string("session_id")?;
    let tool = string("tool_name")?;
    let input = payload
        .get("tool_input")
        .filter(|v| v.is_object())
        .ok_or_else(|| "PermissionRequest needs an object tool_input".to_owned())?;
    let input = serde_json::to_string_pretty(input).map_err(|e| e.to_string())?;
    Ok(NativePermission {
        session_id,
        prompt: format!("Allow {tool}?\n{input}"),
    })
}

pub fn permission_options() -> Vec<DecisionOption> {
    vec![
        DecisionOption {
            id: "once".into(),
            label: "Allow once".into(),
        },
        DecisionOption {
            id: "deny".into(),
            label: "Deny".into(),
        },
    ]
}

pub fn permission_output(option: &str) -> Option<Value> {
    let decision = match option {
        "once" => json!({"behavior":"allow"}),
        "deny" => json!({"behavior":"deny","message":"Denied in signaltty"}),
        _ => return None,
    };
    Some(json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":decision}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_payload_and_verdict_match_the_documented_provider_contract() {
        let payload = json!({"session_id":"s","tool_name":"Bash","tool_input":{"command":"cargo test"},"extra":"ignored"});
        for provider in ["codex", "claude"] {
            let permission = native_permission(provider, &payload).unwrap();
            assert_eq!(permission.session_id, "s");
            assert!(permission.prompt.contains("cargo test"));
        }
        assert_eq!(
            permission_output("once"),
            Some(
                json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}})
            )
        );
        assert_eq!(
            permission_output("deny").unwrap()["hookSpecificOutput"]["decision"]["behavior"],
            "deny"
        );
        assert!(permission_output("always").is_none());
        assert!(native_permission("opencode", &payload).is_err());
        for payload in [
            json!({}),
            json!({"session_id":"s","tool_name":"Bash","tool_input":null}),
            json!({"session_id":"","tool_name":"Bash","tool_input":{}}),
        ] {
            assert!(native_permission("codex", &payload).is_err());
        }
    }
}

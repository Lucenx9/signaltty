//! Small GUI helpers: relative time, CSS classes, stylesheet.

/// "now", "5m", "2h", "3d" from an RFC3339 timestamp. Garbage → "".
pub fn time_ago(iso: &str) -> String {
    let Ok(ts) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let secs = (chrono::Utc::now() - ts.with_timezone(&chrono::Utc))
        .num_seconds()
        .max(0);
    if secs < 60 {
        "now".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// CSS class for an attention state. Empty for none.
pub fn attention_css(attention: &str) -> &'static str {
    match attention {
        "unread" => "attention-unread",
        "input_required" => "attention-input",
        "permission_required" => "attention-permission",
        "warning" => "attention-warning",
        "error" => "attention-error",
        _ => "",
    }
}

pub const CSS: &str = r#"
.pane-frame { border: 2px solid transparent; border-radius: 8px; padding: 1px; }
.pane-frame.attention-unread { border-color: #3584e4; }
.pane-frame.attention-input { border-color: #f5c211; }
.pane-frame.attention-permission { border-color: #ff7800; border-width: 3px; }
.pane-frame.attention-warning { border-color: #e5a50a; }
.pane-frame.attention-error { border-color: #e01b24; border-width: 3px; }
.attention-dot { font-weight: bold; }
.attention-dot.attention-unread { color: #3584e4; }
.attention-dot.attention-input { color: #f5c211; }
.attention-dot.attention-permission { color: #ff7800; }
.attention-dot.attention-warning { color: #e5a50a; }
.attention-dot.attention-error { color: #e01b24; }
.lifecycle-working { color: #33d17a; }
.lifecycle-blocked { color: #f5c211; }
.lifecycle-failed { color: #e01b24; }
.lifecycle-done { color: #c0bfbc; }
.pane-title { font-size: small; }
.dim { color: @theme_unfocused_fg_color; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ago_buckets() {
        let now = chrono::Utc::now();
        let iso = |dt: chrono::DateTime<chrono::Utc>| dt.to_rfc3339();
        assert_eq!(time_ago(&iso(now)), "now");
        assert_eq!(time_ago(&iso(now - chrono::Duration::minutes(5))), "5m");
        assert_eq!(time_ago(&iso(now - chrono::Duration::hours(2))), "2h");
        assert_eq!(time_ago(&iso(now - chrono::Duration::days(3))), "3d");
        assert_eq!(time_ago("garbage"), "");
    }

    #[test]
    fn attention_classes() {
        assert_eq!(attention_css("none"), "");
        assert_eq!(attention_css("error"), "attention-error");
        assert_eq!(attention_css("bogus"), "");
    }
}

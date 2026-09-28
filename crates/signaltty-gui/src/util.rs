//! Small GUI helpers.

use chrono::{DateTime, Utc};

/// "now", "5m", "2h", "3d" since `ts`.
pub fn time_ago(ts: DateTime<Utc>) -> String {
    let secs = (Utc::now() - ts).num_seconds().max(0);
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

/// `$HOME/...` → `~/...` for display.
pub fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ago_buckets() {
        let now = Utc::now();
        assert_eq!(time_ago(now), "now");
        assert_eq!(time_ago(now - chrono::Duration::minutes(5)), "5m");
        assert_eq!(time_ago(now - chrono::Duration::hours(2)), "2h");
        assert_eq!(time_ago(now - chrono::Duration::days(3)), "3d");
        assert_eq!(time_ago(now + chrono::Duration::minutes(1)), "now");
    }
}

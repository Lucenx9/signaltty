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
    tilde_with_home(path, std::env::var("HOME").ok().as_deref())
}

/// Collapse `home` to `~` only when `path` is exactly home or a path under it.
/// A shared string prefix alone is not enough (`/home/sim` must not match
/// `/home/simon/...`).
fn tilde_with_home(path: &str, home: Option<&str>) -> String {
    match home {
        Some(home) if !home.is_empty() && is_under_home(path, home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

fn is_under_home(path: &str, home: &str) -> bool {
    path == home || (path.starts_with(home) && path.as_bytes().get(home.len()) == Some(&b'/'))
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

    #[test]
    fn tilde_requires_path_boundary_not_mere_prefix() {
        assert_eq!(tilde_with_home("/home/sim", Some("/home/sim")), "~");
        assert_eq!(
            tilde_with_home("/home/sim/code", Some("/home/sim")),
            "~/code"
        );
        // Neighbor whose name only shares a prefix must stay absolute.
        assert_eq!(
            tilde_with_home("/home/simon/code", Some("/home/sim")),
            "/home/simon/code"
        );
        assert_eq!(
            tilde_with_home("/home/simx", Some("/home/sim")),
            "/home/simx"
        );
        assert_eq!(tilde_with_home("/tmp/x", Some("/home/sim")), "/tmp/x");
        assert_eq!(tilde_with_home("/home/sim/code", None), "/home/sim/code");
        assert_eq!(
            tilde_with_home("/home/sim/code", Some("")),
            "/home/sim/code"
        );
    }
}

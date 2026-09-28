//! Hook attribution: map a hook client to its pane.
//!
//! Fast path: explicit `pane_id` (shims read $SIGNALTTY_PANE).
//! Fallback: process-ancestry resolution — the hook process is a
//! descendant of the pane's child, so walk /proc parents from the
//! client's pid and match the deepest pane child. This survives
//! sandboxed agents that strip hook env, and safely ignores hooks
//! fired outside our panes (e.g. Cursor Desktop shares hooks.json).

use std::collections::HashMap;

/// Read the parent pid of `pid` from /proc. Linux-only; None elsewhere.
pub fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces/parens: ppid follows the last ')'.
    let after = stat.rsplit(") ").next()?;
    let mut fields = after.split_whitespace();
    fields.next()?; // state
    fields.next()?.parse().ok() // ppid
}

/// Ancestor chain of `pid`, starting with `pid` itself. Capped.
pub fn ancestors(mut pid: u32) -> Vec<u32> {
    let mut out = vec![pid];
    for _ in 0..64 {
        match parent_pid(pid) {
            Some(0) | None => break,
            Some(ppid) => {
                out.push(ppid);
                if ppid == pid {
                    break;
                }
                pid = ppid;
            }
        }
    }
    out
}

/// Resolve `client_pid` to the deepest pane whose child is an ancestor.
pub fn resolve_pane_by_ancestry(
    client_pid: u32,
    child_pids: &HashMap<String, u32>,
) -> Option<String> {
    if child_pids.is_empty() {
        return None;
    }
    let by_pid: HashMap<u32, &String> = child_pids.iter().map(|(id, pid)| (*pid, id)).collect();
    ancestors(client_pid)
        .into_iter()
        .filter_map(|pid| by_pid.get(&pid))
        .next()
        .map(|s| (*s).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestors_include_self_and_init() {
        let me = std::process::id();
        let chain = ancestors(me);
        assert_eq!(chain[0], me);
        assert!(chain.len() > 1);
        assert_eq!(*chain.last().unwrap(), 1);
    }

    #[test]
    fn resolves_deepest_pane_child() {
        let me = std::process::id();
        let chain = ancestors(me);
        let parent = chain[1];
        let mut map = HashMap::new();
        map.insert("pane_outer".to_string(), 1);
        map.insert("pane_inner".to_string(), parent);
        // Deepest match wins: parent beats init.
        assert_eq!(
            resolve_pane_by_ancestry(me, &map),
            Some("pane_inner".to_string())
        );
        assert_eq!(resolve_pane_by_ancestry(me, &HashMap::new()), None);
    }
}

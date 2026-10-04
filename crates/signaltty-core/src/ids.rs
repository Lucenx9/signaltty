//! Opaque stable IDs: UUIDv4 with a cosmetic type prefix.
//! Consumers must treat them as opaque strings.

use uuid::Uuid;

fn new_id(prefix: &str) -> String {
    format!("{}_{}", prefix, Uuid::new_v4().simple())
}

pub fn new_ws_id() -> String {
    new_id("ws")
}
pub fn new_tab_id() -> String {
    new_id("tab")
}
pub fn new_pane_id() -> String {
    new_id("pane")
}
pub fn new_notif_id() -> String {
    new_id("notif")
}
pub fn new_task_id() -> String {
    new_id("task")
}
pub fn new_context_id() -> String {
    new_id("tctx")
}

pub fn has_prefix(id: &str, prefix: &str) -> bool {
    id.starts_with(&format!("{prefix}_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_have_prefixes_and_unique() {
        let a = new_pane_id();
        let b = new_pane_id();
        assert_ne!(a, b);
        assert!(has_prefix(&a, "pane"));
        assert!(has_prefix(&new_ws_id(), "ws"));
        assert!(has_prefix(&new_tab_id(), "tab"));
        assert!(has_prefix(&new_notif_id(), "notif"));
        assert!(has_prefix(&new_task_id(), "task"));
        assert!(has_prefix(&new_context_id(), "tctx"));
    }
}

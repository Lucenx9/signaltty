# Verification and use

Use menu/shortcuts to open Command Palette, Rename Workspace, Search Terminal, Zoom Pane, Worktrees and Show Changes. Palette executes existing actions or navigates to a named workspace. Search treats text literally. Zoom toggles current pane, preserving saved splits.

Use `signaltty worktree list/create/open/remove` with a workspace handle and explicit checkout path. Create uses a new branch. Close the worktree workspace before removal; dirty/main/live paths are refused and branches retained.

Start supported agents directly to prepare native permission hooks. PermissionRequest produces Allow once/Deny controls, with timeout/cancellation falling back to provider UI. Codex still requires native hook trust. All reporter verification uses temporary roots and real PTYs.

Gates: cargo build --workspace; cargo test --workspace; cargo clippy --workspace --all-targets; cargo fmt --check. Run new GTK display tests individually with dbus-run-session. Capture and inspect light/dark isolated scenes; do not reuse a user's running server/window.

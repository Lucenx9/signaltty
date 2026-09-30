# Local agent workflows — 2026-09-30

Feature: [013-local-agent-workflows](../../specs/013-local-agent-workflows/spec.md),
branch `013-local-agent-workflows`, base `2255f26`. Browser and SSH excluded.
Implementation commits: `64d557f` (server/reporters/CLI) and `f98c962` (GUI).
Verification artifacts cover both.

## Delivered behavior

Native Claude/Codex PermissionRequest reporters return Allow once/Deny JSON through
a live request-owned route. Cancellation, session changes, deadlines and restart
cannot restore or deliver a grant. Native provider trust remains user-owned.

The GTK palette, rename, literal terminal search and temporary pane zoom preserve
VTE identity/output and server divider ratios. Worktree creation/open/removal use
real Git registrations, reuse canonical checkout workspaces and keep branches.
Dirty/main/locked/open/live targets refuse removal. Normal launch/creation and
removal share path reservations. Git hooks are killed with their process group
on timeout/cancellation. A timed-out create can leave checkout/branch state and
reports that uncertainty without automatic retry.

Working Tree Changes shows aggregate/per-file counts against HEAD, including
binary and untracked labels. Literal filenames survive; renames display as
delete/add. No per-turn claim.

## Verification

- `cargo fmt --check`, `cargo build --workspace`: pass.
- `cargo test --workspace`: **231 passed**, 14 display tests intentionally ignored.
- All **14 GTK display tests** run separately under D-Bus: pass.
- `cargo clippy --workspace --all-targets`: pass with only the existing
  `audit.rs` open-options and `router.rs` workspace-list sorting warnings.
- Seven native permission tests execute installed reporters in actual managed
  PTYs and observe exact verdict stdout and no terminal input.
- Seven real-Git IPC/CLI tests establish filesystem isolation/reuse, restore,
  dirty/main/locked/live refusal, branch retention and concurrent launch safety.
- Two real repository-hook regressions establish process-group cleanup on
  deadline and cancellation. Four Git diff tests include literal tab/newline
  filenames and unborn HEAD.
- Slow worktree IPC test receives a reply after the ordinary 3s timeout while
  normal control remains available; worktree operations use a separate connection.
- Server edge-case QA and refresh benchmark: pass.
- Native AT-SPI GUI reliability QA: pass, covering activation, decision focus/answer,
  reconnect output, responsive GTK during a stalled server, split/close and restore.

The desktop's stale AT-SPI bus initially refused connections. The successful
reliability run used an owned accessibility D-Bus daemon with its activation
environment pointed to its own `AT_SPI_BUS_ADDRESS`, plus an isolated session
bus. It did not restart or change desktop services. Owned processes were stopped
after the run.

The Standards review found a palette widget reference cycle; weak query capture
and an actual destruction assertion fix it. The independent Spec/adversarial
review reproduced orphan Git hooks after timeout; process-group cleanup and
regressions fix that. No unresolved blocking finding remains.

## Evidence and limits

[Raw proof artifacts](local-workflows-2026-09-30/) contain gate logs, real server
and GUI QA JSONL, benchmark results and 12 inspected GTK screenshots.

| Surface | Light | Dark |
|---|---|---|
| Palette | [image](local-workflows-2026-09-30/palette-light.png) | [image](local-workflows-2026-09-30/palette-dark.png) |
| Worktrees | [image](local-workflows-2026-09-30/worktrees-light.png) | [image](local-workflows-2026-09-30/worktrees-dark.png) |
| Changes | [image](local-workflows-2026-09-30/changes-light.png) | [image](local-workflows-2026-09-30/changes-dark.png) |
| Search | [image](local-workflows-2026-09-30/search-light.png) | [image](local-workflows-2026-09-30/search-dark.png) |
| Zoom | [image](local-workflows-2026-09-30/zoom-light.png) | [image](local-workflows-2026-09-30/zoom-dark.png) |

Narrow 360px [worktree](local-workflows-2026-09-30/worktrees-narrow.png) and
[changes](local-workflows-2026-09-30/changes-narrow.png) dialogs were also inspected.
Screenshots render actual GTK windows; dialog tests supply responses at the
production actor seam. Separate real-Git tests prove the filesystem effects.

Provider approval verdicts are verified against the documented Claude/current
Codex0.159.1 contracts and installed reporter commands. This does **not** certify
a paid model session consuming the verdict inside its native consent UI. No paid
provider calls or personal provider configuration changes were made. Clipboard,
IME and folder chooser interactions are outside this increment's proof.

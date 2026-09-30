# Implementation plan: Local agent workflows

**Branch**: `013-local-agent-workflows` | **Date**: 2026-09-30 | **Spec**: [spec.md](spec.md)

## Summary

Connect native Claude/Codex PermissionRequest hooks to existing decisions using a transient response broker. Add GTK command/workspace palette, rename, literal terminal search and view-only pane zoom. Add real Git worktree lifecycle via typed IPC/CLI and GTK create/open/list plus on-demand change summaries.

## Technical context

Rust edition 2021, minimum 1.85. Existing Tokio/serde IPC, GTK4/libadwaita/VTE only in GUI, Git executable via argv. Snapshot persists workspace structure; native verdict channels and zoom/search are ephemeral. Linux desktop and single-user local server. No new network/provider service or dependency family.

## Constitution check

- Spec-first: spec and complete requirements checklist precede implementation; tasks follow this plan.
- Product bar: actual attention/approval and work isolation; unsupported native channels remain honest. Browser/SSH excluded by user.
- Skills: Specify/domain modeling, Architect/arena two candidates, Implement/TDD/karpathy/boundary discipline, native frontend design and Emil, review/code-review/interrogate, prove-it-works.
- Testing seams already prescribed by project: core unit, typed IPC with real PTYs and temporary Git repositories, CLI reporter round trip, actual GTK controls.
- Deep modules: server approval/worktree modules own their policy; GUI palette/dialogs own presentation. Router/App remain coordinators. Core/proto toolkit-free.
- ADR records native response route, lifetime and Git checkout safety. No speculative compatibility services or permanent approval policy.

## Grounding

Installed reporter -> CLI HookEvent -> router adapter -> Store decision -> broadcast -> persistent GTK bar -> decision.answer. Current static answer channel injects numeric PTY bytes; native waiting requests need their own live route. Server connection dispatch currently blocks that connection's EOF observation. Store owns decision transitions/audit, not OS transports.

App caches workspaces and keyed PaneWidgets, projects Layout into split containers and keeps VTE objects alive. Ratio updates move existing Paned widgets. Search belongs to PaneWidget; zoom changes effective view layout only. Palette invokes existing GActions. Rename and diff reuse current IPC.

Git registrations own worktree identity. Workspaces associate canonical checkout paths, not duplicated checkout lifetime. Closing GUI/workspace does not remove Git files. Workspace creation and pane launch can race worktree removal, so checkout removal must reserve/recheck paths across those boundaries.

## Sketch and module ownership

- `signaltty-agent`: pure PermissionRequest input conversion and native verdict codec, Allow once/Deny only.
- `signaltty-server/src/approvals.rs`: transient broker, registered waiter plus cancellation guard; one current decision per pane, consume once and no auto-grant. Waiting owned by dedicated CLI connection; timeout below provider125s, select EOF/shutdown.
- `signaltty-server/src/worktrees.rs`: list/create/open/remove, canonical Git membership, mutation serialization/removal reservation, normal no-force Git operations. Never remove main/dirty/live-referenced checkout; retain branch.
- `signaltty-cli/src/permission_hook.rs`: dedicated native stdout verdict; ordinary hook reporting stays silent. Worktree CLI thin typed commands.
- `signaltty-gui/src/palette.rs`, `workspace_dialogs.rs`: native palette and dialogs. App supplies selected identities and existing async actor.
- `PaneWidget`: literal VTE search, next/previous/close. Client-local zoom restores latest full layout and retains hidden widgets.

## Project structure

Feature documents: spec, plan, research, data-model, contracts, quickstart, tasks and requirements checklist.
Code changes: the server/agent/integration/CLI/GUI/proto crates only. Tests at IPC/reporter/GTK boundaries, isolated state and directories.

## Implementation strategy

Independent verifiable slices for US1, US2 and US3. Parallel file ownership prevents competing edits; shared router/proto/CLI registration is serialized. Each slice writes a red behavior test before implementation. Parent integrates and runs complete gates once final behavior is stable. No changes to running user sessions or personal provider configuration during verification.

## Complexity tracking

Native response broker is necessary because static provider kind cannot prove a live permission receiver. Worktree reservation is necessary because filesystem removal and concurrent pane launches share checkout state. Neither is a general workflow framework.

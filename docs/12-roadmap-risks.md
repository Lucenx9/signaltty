# 12 — MVP Roadmap & Known Technical Risks

## Roadmap

### Phase 0 — Research & architecture (this deliverable) ✅

Upstream inspection (Ghostty/libghostty-vt, Codex/Claude/OpenCode/
Cursor CLIs + hooks, OSC 9/99/777, `portable-pty`/`vt100`/
`alacritty_terminal`/`wezterm-term`, gtk4-rs/`vte4`, cxx-qt/Qt6,
`notify-rust`/`ashpd`, XDG), docs 01–12, ADRs. Exit criteria: every
choice below cites a verified upstream fact, not an assumption.

### Phase 1 — Multiplexer core ✅ done

Rust workspace (`core, proto, term, server, cli, testkit`); server
owns PTYs via `portable-pty`; JSONL Unix-socket IPC `signaltty/1`;
workspaces/tabs/panes/splits; attach/detach; bounded scrollback +
`vt100` headless screen; snapshot persistence + restore states;
CLI with JSON output (`new, workspace, tab, pane, read, input,
notify, wait, status, daemon`). Exit criteria: useful as a
lightweight multiplexer; GUI closure/crash never kills panes;
integration tests with real PTYs green.

### Phase 2 — Agent awareness ✅ done

`signaltty-agent`: `AgentAdapter` trait + Codex/Claude/OpenCode/
Cursor/Generic impls; hook shims + `integration install`; OSC 9/99/777
scanner wired to attention; explicit `notify`/`hook-event`/
`report-session`; lifecycle × attention engine; resume argv builders.
Exit criteria: `codex`/`claude` turn end, permission request, and
question states detected semantically (hooks), with OSC/process/title
fallbacks; session ids persisted; one-key resume works per adapter.

### Phase 3 — Native GUI ✅ done

`signaltty-gui` (plain gtk-rs + gtk4 + libadwaita +
`vte4`-as-renderer; no relm4): sidebar
(project/agent/doing/needs-me), splits, attention rings, next-unread
jump, Git badges, desktop notifications with focus-on-click. GTK4
`v4_18` feature required (unblocks vte4's `Accessible` impl);
glib 0.22 has no `MainContext::channel` (tokio mpsc + `spawn_local`
instead) and gtk4 0.11 has no size-allocate signal (250ms
column/row sync tick instead). Requires GTK4/libadwaita/VTE system
deps. Verified headless (Xvfb + screenshots): render, splits, live
streaming, input roundtrip, focus-clears-attention, detach survival.

### Phase 4 — Extensibility ✅ done

Executable-plugin manifests (`plugin.toml`) + event hooks (JSON on
stdin, bounded concurrency, timeouts, `plugin.list`/`plugin.reload`)
+ `plugin run` commands + workflow recipes
(`examples/plugins/`, `examples/workflows/fanout-review.sh`) +
orchestration via semantic calls (new/split/wait/read/notify).
Documented trust model (runs as user, no sandbox v1). Deferred to
future work: read-only automation tokens, sandboxing spike.

### Shipped since

- `001-priority-sorted-sidebar`
- `002-self-printing-schema`
- `003-inline-approvals`
- `004-close-workspace-gui`
- `005-agent-manifests-proc`
- `006-agent-skill-handles-audit`
- `007-notif-actions-diff`
- `008-qa-reliability`
- `009-ui-refinement`
- `010-automatic-agent-hooks`
- `011-codex-pane-runtime`
- `012-workspace-visual-hierarchy`
- `013-local-agent-workflows`
- `014-file-diff-review`
- `015-agent-ready-repository`
- `016-agent-event-recovery`
- `017-themes`
- `018-orchestrator`
- `019-task-board`


## Known technical risks

| # | Risk | Likelihood / Impact | Mitigation |
|---|---|---|---|
| 1 | Hook/payload drift across agent CLI releases | High / Medium | Adapters ignore unknown fields; per-adapter version probes; Generic fallback always works; integration tests pin adapter fixtures per CLI version |
| 2 | Resumed sessions emit no identity event (observed) | High / Low | `report-session` CLI + env injection + last-known-id retention |
| 3 | `vt100` too lossy for some TUIs (alt-screen, attrs) | Medium / Low | OSC/title/exit semantics don't depend on it; swap to `alacritty_terminal` behind `TerminalBackend` |
| 4 | VTE-as-external-renderer friction (feed/input mapping) | Medium / Medium | Spike early in Phase 3; fallback to custom renderer over headless state |
| 5 | Multi-viewer resize contention | Medium / Low | Last-writer-wins + broadcast (documented in 04); revisit tiling hints if painful |
| 6 | Scrollback memory with many panes | Medium / Medium | Hard per-pane caps + tail-only persistence; metrics in `server.status` |
| 7 | GTK system-dep availability on user machines | Low / Medium | Core/CLI fully usable without GUI; desktop entry and icon install to `~/.local` for systems with GTK; assess Flatpak packaging later |
| 8 | Agent CLIs changing resume flags | Medium / Medium | `resume_capability` owned per adapter, probed (`--help`) at install; never auto-run without opt-in |
| 9 | Notification fatigue / false attention | Medium / High | Explicit-beats-heuristic ordering; per-pane mute; attention only re-raises on new signals |
| 10 | libghostty-vt temptation (unstable API) | Low / High | Pinned behind trait + feature flag only; never on the critical path |

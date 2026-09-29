# 11 — Crate & Module Layout

Cargo workspace, Rust 2021+, MSRV 1.85 (matches `alacritty_terminal`
floor if later adopted; `vt100` needs only 1.70).

```text
Cargo.toml                  # workspace
crates/
  signaltty-core/           # toolkit-free domain model (no tokio, no PTY)
    src/{lib,ids,model,state,layout,notification,git,error}.rs
  signaltty-proto/          # IPC envelopes, methods, events, versions
    src/{lib,protocol,message,method,event,error}.rs
  signaltty-term/           # TerminalBackend trait + headless impl + OSC scan
    src/{lib,backend,headless,osc,sanitize}.rs
  signaltty-agent/          # Phase 2: AgentAdapter trait + impls + hooks
    src/{lib,types,registry,adapters/*}.rs
  signaltty-integration/    # OS-facing hook configuration/transactions (ADR-0013)
    src/lib.rs
  signaltty-server/         # daemon: store, PTY mgr, router, persist, bus
    src/{main,server,store,pty,router,events,persist,config,gitwatch}.rs
  signaltty-cli/            # clap client (human + automation + hooks entry)
    src/{main,client,daemon,attach,integration}.rs
  signaltty-plugin/         # Phase 4: plugin.toml manifests + hook dispatch
    src/lib.rs
  signaltty-gui/            # Phase 3: gtk4 + libadwaita + vte4 (plain gtk-rs)
    src/{main,app,actor,sidebar,terminal,notif,util}.rs
  signaltty-testkit/        # shared integration-test harness (real PTYs)
    src/{lib,server,pty_helpers}.rs
```

## Dependency rules

- `core`: `serde, uuid, chrono|time, thiserror` only. No async, no OS.
- `proto` → `core`. `term` → `core` (+`vt100`, `vte`).
- `server` → `core, proto, term, agent, integration` (+`tokio, portable-pty`,
  `serde_json`, `nix`, `base64`).
- `cli` → `core, proto, integration` (+`tokio, clap, serde_json`). Never `server`.
- `gui` → `core, proto` (+`gtk4, libadwaita, vte4`,
  `notify-rust`). Never `server`, never PTY crates.
- `plugin` → `core, proto` (+`tokio, toml`). `server` and `cli`
  depend on it; it never depends on them.
- `agent` → `core, proto` only; adapters are pure classifiers over
  `AdapterEvent` snapshots so they stay unit-testable without PTYs.

- `integration` → `serde, serde_json, thiserror, nix` only; filesystem hook
  installation shared by server/CLI, never GTK or server dependencies.

## Binaries

`signaltty-server` (daemon), `signaltty` (CLI), `signaltty-gui`
(Phase 3). One version for the whole workspace; `server.status`
reports it and IPC refuses cross-major mismatches.

## Phase 1 build order

`core → proto → term → server → cli → testkit`, each with unit tests;
integration tests live in `server/tests/` + `cli/tests/` using
`testkit` with real PTYs.

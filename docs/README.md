# signaltty documentation

The server owns sessions and processes. GTK and CLI are clients. Start with the current architecture or the development workflow for your task. Research and verification reports retain their original dates and describe the checkout they examined.

## Current architecture and product direction

- [01 — Architecture & process model](01-architecture.md)
- [02 — Data model (server/workspace/tab/pane)](02-data-model.md)
- [03 — Lifecycle & attention models](03-lifecycle-attention.md)
- [04 — PTY lifecycle](04-pty-lifecycle.md)
- [05 — Terminal backend research](05-terminal-backend.md)
- [06 — GUI toolkit decision (GTK vs Qt)](06-gui-toolkit.md)
- [07 — Agent integration strategy](07-agents.md)
- [08 — IPC schema](08-ipc.md)
- [09 — Persistence & restore](09-persistence.md)
- [10 — Security model](10-security.md)
- [11 — Crate layout](11-crates.md)
- [12 — MVP roadmap & risks](12-roadmap-risks.md)
- [13 — Plugins & workflows](13-plugins.md)
- [14 — Product direction (cmux + herdr, t3code visual bar)](14-product-direction.md)
- [16 — Visual references (Geist, Linear, Vercel)](16-visual-references.md)

## Development workflow

- [15 — Agent skills (install + phase routing)](15-agent-skills.md)
- [17 — Agent development and verification](17-agent-development.md)
- [18 — Agent workflow commands and source ownership](18-agent-workflow.md)
- [19 — CI and pull request delivery](19-ci-and-pull-requests.md)
- [Feature catalog and recorded lifecycle](../specs/README.md)

## Architecture decisions

- [ADRs](adr/)

## Research

- [License provenance and remaining distribution checks, 2026-10-11](research/2026-10-11-license-provenance.md)
- [Agent Orchestrator review, 2026-10-12](research/2026-10-12-agent-orchestrator.md)
- [Agent-ready repository research, 2026-10-03](research/2026-10-03-agent-ready-repository.md)
- [Agent workflow automation research, 2026-10-14](research/2026-10-14-agent-workflow-automation.md)
- [cmux and Herdr source review, 2026-10-03](research/2026-10-03-cmux-herdr-code.md)

## Historical verification evidence

- [App verification, 2026-09-29](qa/2026-09-29.md)
- [Native UI refinement verification, 2026-09-29–30](qa/ui-2026-09-29.md)
- [Workspace visual hierarchy verification, 2026-09-30](qa/visual-hierarchy-2026-09-30.md)
- [Automatic hook configuration verification, 2026-09-30](qa/automatic-hooks-2026-09-30.md)
- [Codex pane runtime verification, 2026-09-30](qa/codex-pane-runtime-2026-09-30.md)
- [Local agent workflow verification, 2026-09-30](qa/local-workflows-2026-09-30.md)
- [Per-file diff review verification, 2026-10-03](qa/file-diff-review-2026-10-03.md)

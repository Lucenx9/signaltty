# signaltty — Architecture Deliverable (Phase 0)

First deliverable for the native Linux multi-agent workspace.
Covers: architecture diagram, process/server model, data model,
lifecycle/attention models, PTY lifecycle, terminal-backend research,
GTK vs Qt, agent integration, IPC schema, persistence, security,
crate layout, roadmap, risks.

Design rule: **the server owns the processes, the terminal backend
renders terminal state, agent adapters provide semantic agent state,
notifications describe human attention, and the UI helps the user
find the work that needs them.**

Docs index:

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
- [15 — Agent skills (install + phase routing)](15-agent-skills.md)
- [16 — Visual references (Geist, Linear, Vercel)](16-visual-references.md)
- [App verification, 2026-09-29](qa/2026-09-29.md)
- [Native UI refinement verification, 2026-09-29–30](qa/ui-2026-09-29.md)
- [Workspace visual hierarchy verification, 2026-09-30](qa/visual-hierarchy-2026-09-30.md)
- [Automatic hook configuration verification, 2026-09-30](qa/automatic-hooks-2026-09-30.md)
- [Codex pane runtime verification, 2026-09-30](qa/codex-pane-runtime-2026-09-30.md)
- [Local agent workflow verification, 2026-09-30](qa/local-workflows-2026-09-30.md)
- [ADRs](adr/)

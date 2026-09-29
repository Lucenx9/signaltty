# Implementation Plan: Workspace reliability fixes

## Design decision

Use asynchronous `IpcHandle::call` futures on GTK's local executor, retaining the existing oneshot completion channel. Enqueue attach requests and deliver both initial and reconnect snapshots through the same UI FIFO as PTY data. Bound socket operations and request completion; discard uncertain connections without replaying mutations. Serialize cache refresh batches and never hold a RefCell borrow across an await.

Two independent designs considered a callback registry and local futures. Cross-review chose futures: fewer lifetime/completion types and natural composition of existing multi-request actions. Keep the callback design's consistent acknowledgment semantics and authoritative pane ownership cleanup.

Core validates unique layout leaves and finite divider ratios. The server validates exact membership and workspace parentage, stages automatic tabs until spawn succeeds, and closes all owned panes. GTK presentation uses effective lifecycle derived from live process state. Activation presents the existing window.

## Boundaries and constraints

No GTK/OS dependencies in core. No new agent/provider calls. Preserve persistent VTE widgets on ratio changes and reconnect. Acknowledgment preserves unresolved decisions; ADR-0012 documents the intentional contract correction. Inspect snapshot/live-output ordering with concurrent output; extend the protocol only if needed to remove overlap.

## Verification

First reproduce at the owning seam: integration IPC tests for server invariants, fake socket actor tests for timeouts/snapshot ordering, pure status tests and display regression tests. Re-run the original QA script, stalled GTK action and reconnect relay probes, activation count and restored status. Capture light/dark GUI evidence. Run build, workspace tests, explicit display tests, clippy, fmt and refresh benchmark. Review final diff before commits and direct push to main.

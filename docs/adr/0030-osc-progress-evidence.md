# ADR-0030: Retain OSC progress as PTY-local detection evidence

Status: proposed, 2026-10-11. Spec: `specs/045-osc-progress-grok/`.

## Context

Grok emits OSC 9;4 busy/idle signals used by herdr's declarative manifest.
Treating these as ordinary OSC 9 messages creates notifications containing
protocol text. Mapping progress directly to lifecycle would bypass program
scoping, visible blocker priorities, user overrides and semantic hook authority.

## Decision

The scanner emits Progress separately from Notify and retains the validated
payload after `9;`. States are 0..4, with optional/empty percentage or -1..100.
The -1 sentinel supports Grok. Evidence is at most 32 ASCII bytes; malformed
reserved progress is ignored. Ordinary OSC 9 text and bare BEL are unchanged.

Each PTY owns its scanner. The classifier and explain read the current handle's
progress through an explicit matcher input and `osc_progress` region. Missing
progress never matches. Exit/close drop the handle and respawn creates a fresh
scanner, so evidence cannot survive a cold restart or PTY replacement. Progress
does not add persisted state or per-update events.

Bundle Grok under generic `binaries = ["grok", "grok-build"]`. Port herdr
2563803's priorities, including visible blockers over busy progress and visible
work over stale idle titles. Reuse existing known-agent idle fallback, launcher
discovery, user replacement rules and Store lifecycle transitions.

## Consequences

Other agents can use this region in future manifests. This PR adds Grok only.
No native Grok hooks or resume installer is implied. Progress is best-effort
retained terminal evidence, not a completion authority. Programs started within
an existing shell use the existing process refresh/scoping behavior.

Sources: [herdr Grok manifest](https://github.com/herdrdev/herdr/blob/2563803/src/detect/manifests/grok.toml),
[ConEmu OSC reference](https://conemu.github.io/en/AnsiEscapeCodes.html#ConEmu_specific_OSC).

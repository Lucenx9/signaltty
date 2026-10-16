# Plan: Native harness task interruptions

Reuse adapter lifecycle classification, Store transitions and existing task IPC.
After hook decision ingestion, park an unstructured blocked worker through the
existing Store transition. Extend current blocker reconciliation to hooked panes;
keep native structured decisions first. Resolve hook-attributed interruptions
when an explicit new prompt/finished turn arrives, and order delivered-answer lifecycle
updates so a consumed native decision is not mistaken for a new hook blocker.

Preserve automatic launch flags, explicit argv semantics, hook authority and
screen fallback rules. Document unsupported permission telemetry as an audit
limit, not a fixed provider contract. No new IPC methods, state schema or GUI.

Constitution: clarified spec before implementation, existing public regression
seams, mutate/emit pairing, independent reviews and full shared verification.
Run targeted orchestrator/native permission/Store regressions before
scripts/verify.sh full. Keep evidence under ignored target/verification.

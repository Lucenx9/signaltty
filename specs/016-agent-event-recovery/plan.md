# Implementation plan

## Grounding

The authoritative store allocates sequences but starts at zero. State events enter
both a bounded ring and the audit; PTY events consume sequence numbers without
entering either. Publishers can send after unlocking, so wire order is not guaranteed.
Subscription acknowledgment currently precedes replay capture; lag is discarded.
The actor already reconnects, reloads workspace state and replaces screens in existing
terminal widgets. Output offsets trim overlap but currently tolerate missing bytes.
Wait checks current state only; a previous Done immediately satisfies new work.

Read docs 02, 03, 07, 08, 09, 14, 15, 17, the constitution and verification skill.
Upstream source evidence is in docs/research/2026-10-03-cmux-herdr-code.md.
Independent designs and their synthesis are recorded in research.md.

## Interfaces and ownership

- Audit owns durable scalar sequence reservations and retention evidence. Reservations
  are persisted before use; crashes leave unused numbers. Corrupt reservations fail
  startup. An allocation failure stops event issuance rather than reusing identifiers.
- Store owns the existing state-event ring and runtime pane progress. Lifecycle and
  attention transitions stamp their emitted sequence independently. Opaque process
  identity changes on spawn/resume, remains on exit and is never restored from disk.
- `pane.get` adds `wait_baseline`; wait accepts an optional `after` baseline and a
  string or nonempty list of outcomes. Identity mismatch precedes state evaluation.
- Subscribe captures acknowledgment, completeness and replay under one store lock.
  Complete replay uses a frozen handoff fence; incomplete history returns no prefix
  and requests snapshot recovery. Only matching events count against the replay cap.
- Connection handling suppresses pre-fence state events, closes on lag/out-of-order
  events, and cancels waits on EOF/shutdown. PTY snapshots and offsets cover output.
- Actor treats forward byte gaps as connection failure and reattaches without
  overwriting the terminal's current size. Existing screen replacement keeps identity.
- CLI, protocol schema, IPC docs and agent instructions expose the same optional fields.

## Constitution check

Spec and clarified requirements precede this plan and tasks. Test-first regressions
use real isolated sockets and the headless actor seam. Core stays pure and toolkit-free;
no persisted pane schema change. State/event pairing remains in Store transitions.
No product-direction deviation. ADR-0018 records cursor/recovery and baseline choices.

## Validation

Restart and crash sequence uniqueness; corrupt reservation; retention/filter/cap
coverage; replay/live fence; lag/out-of-order recovery; screen overlap/gap and resize;
old Done, fast completion, alternatives, process/session replacement, mistyped tokens,
EOF/shutdown cancellation and ordinary wait compatibility. Run scripts/verify.sh full,
inspect native renders and perform independent diff review before publishing the PR.

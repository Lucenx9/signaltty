# Standards and adversarial review

Baseline: `edf10ce`. Focused verification passed: 6 core diff tests and 9 public server integration tests.

## Act on

1. **Warning — stale empty-state row survives reconciliation.** In `crates/signaltty-gui/src/changes.rs:366-370`, `No working tree changes` is removed only when it happens to be row 0. Repro: render an empty summary, then refresh with `-option`; the sort function puts `-option` before the sentinel, so the dialog displays both a real file and “No working tree changes.” This is an actionable correctness bug in the documented structural reconciliation path (`docs/06-gui-toolkit.md`). Track the sentinel directly or remove it independently of sorted position, and cover empty → punctuation-leading filename.

2. **Warning — filesystem failures cross the IPC boundary as successful content.** `file_diff.rs:173-220` returns every checkout-open, permission, metadata, and `read_to_end` failure as `Err(String)`; `read_inner` at lines 162-164 converts all of them into `DiffContent::Unavailable`. `docs/08-ipc.md` explicitly requires read failures to use `IO_ERROR`, reserving unavailable content for unsupported/refused previews. This also breaches the constitution’s typed-boundary rule: one string channel conflates domain refusal with operational failure. Use a small typed snapshot outcome and propagate actual I/O errors as `ParamError`; add a permission/read-error IPC assertion.

3. **Warning — the GUI rewrites truthful truncation metadata.** `changes.rs:459-470` labels every `truncated=true` result “size limit reached,” then appends the server notice. A real 10,001-line response therefore shows a false size-limit cause plus a duplicate “size or line limit” notice. Render the supplied notice once, with one generic incomplete-preview fallback only when absent. The current GTK fixture uses `notice:null`, so it misses the production payload shape.

## Structural/lifetime result

No additional actionable smell or GTK lifetime leak was found. The new 511-line dialog controller is cohesive and below the review threshold; callbacks capture `Weak`, the dialog-owned keepalive is released on close, global style handlers are disconnected, and pending replies are gated by alive/generation/path checks.

## Resolution recheck

All three findings are resolved at the correct boundaries. Summary reconciliation now scans and removes the named sentinel independent of sort order, with an actual GTK empty → `-option` regression. The reader displays the server notice once and uses a truthful generic fallback only when absent. `SnapshotError` now separates policy unavailability from operational I/O, which propagates as `IO_ERROR`; the chmod-000 public IPC regression covers it. `GIT_DIFF_OPTS` is also removed before Git execution, preserving the fixed three-line context contract. No remaining finding.

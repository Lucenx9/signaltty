# Agent workflow CLI contract

scripts/agent.sh launches Python at the checkout root. Commands are status,
context, next, check, index and handoff. --feature accepts a complete slug or a
direct specs child; explicit selection precedes SPECIFY_FEATURE_DIRECTORY and
.specify/feature.json and is never persisted. --task selects a canonical route.
--evidence accepts a report directory or summary.json relative to the checkout.

JSON envelope: schema_version=1, command, ok, data for successful parsing and
findings. Findings carry code, message, path when applicable and action.
Malformed CLI arguments return INVALID_ARGUMENTS with command=null. Invalid
metadata or filesystem input returns INVALID_INPUT. Exit codes are 0 success,
1 failed gate, 2 invalid input.

status exposes checkout, Git facts, feature metadata/artifacts/task observations,
evidence status and next action. context exposes canonical source roles and
chosen routes. next actions distinguish shell argv, agent command names and manual
artifact correction. Running or failed verification returns manual inspection
or diagnosis guidance, not another test command. No command automatically
executes an LLM skill or test run.

check phases: repository, planning, implementation, delivery. Repository checks
are independent of selected feature and historical phase readiness. Feature
phase gates require explicit enrollment and implementation requires the
recorded requirements review checklist. --scope docs-only/one-line declares a
spec-flow exemption but does not waive repository integrity or full delivery proof.

index without --write validates the generated specs index and routing block;
--write atomically regenerates them. handoff exclusively creates a new output,
defaulting to ignored target/handoffs; --notes supplies accepted decisions and
ownership. It reports evidence again after writing. It does not update task or
lifecycle metadata.

Evidence states: fresh, stale, unverified, failed, running, unavailable. Fresh
requires a versioned successful full/desktop report with all completed gates and the complete unique sequence matching display_tests,
valid matching before/after provenance and matching current root/content/index.
Revision is descriptive; identical staged/working content after a commit remains
applicable. Doctor and fast never establish delivery proof.

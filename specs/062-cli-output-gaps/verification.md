# Verification: CLI output gap detection

## Diagnosis

The real CLI regression failed twice before the fix in approximately three seconds: snapshot BEFORE through byte 6, then AFTER ending at byte 12, produced BEFOREAFTER and attach stayed open. This is one missing byte and one output frame, with no live server timing dependency.

Ranked hypotheses were missing gap validation, incorrect snapshot cursor initialization, and swallowed attach-loop errors. Inspection showed unseen_output computed overlap but never rejected a start beyond the cursor. Adding that check before cursor mutation makes the same real CLI regression pass, while contiguous and overlapping output still print BEFOREAFTER exactly once. The error returns through existing terminal-mode restoration. No debug instrumentation was added.

## Executed checks and review

- `cargo test -p signaltty-cli --test cli cli_attach_rejects_output_gaps -- --exact --nocapture`: red twice before fix, green after fix.
- `cargo test -p signaltty-cli --bin signaltty attach::tests -- --nocapture`: both existing tests pass, including legacy no-offset delivery.
- `python3 crates/signaltty-cli/tests/attach_stream.py target/debug/signaltty`: passes after final fixture correction.
- The fixture also passes with a temporary 100 ms scheduling delay before waitpid. That probe was not retained.
- Independent Standards review found one test-output drain race, corrected and re-reviewed with no remaining findings. Independent Spec review found no issues.

## Delivery proof

Run `scripts/verify.sh full --output target/verification/cli-output-gaps-full` against final staged content, then `scripts/agent.sh check --phase delivery --feature 062-cli-output-gaps --evidence target/verification/cli-output-gaps-full`. The report records the result and source identity; this document does not substitute for a passing fresh report. GitHub must additionally pass the latest PR workflow's workflow-lint, rust and minimum-rust jobs for reviewed head and base before merge.

No GUI behavior changed. Real compositor, IME, accessibility and paid provider checks are outside this CLI correction. Preventing this class of bug requires checking stream loss at every consuming client; the regression now covers the CLI alongside existing GUI recovery tests.

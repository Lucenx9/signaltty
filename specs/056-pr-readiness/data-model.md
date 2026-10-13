# Data model: Verified PR readiness

TaskPr gains optional `head_sha`, optional `draft`, typed `mergeability` (unknown, mergeable, conflicting), typed `merge_state` (unknown, clean, blocked, behind, dirty, draft, has_hooks, unstable), optional `refresh_attempted_at`, and optional `refresh_error`. Missing fields deserialize conservatively. `checked_at` remains the last successful observation time.

Every refresh result matches one PR URL and one attempt time, captured when that PR's command starts. A failure changes attempt time and error only. A successful response replaces all observed fields, records its completion time in `checked_at`, and clears the error. An older attempt cannot replace a newer attempt. Store transitions emit task updates and snapshots persist the facts.

TaskStage is derived and never persisted. Ready observations expire when `now - checked_at >= 300 seconds`, and negative ages are invalid. Stage precedence retains disposition/cancellation, terminal PR state, actionable PR failure, ready PR, remaining PR review, and existing task-only states.

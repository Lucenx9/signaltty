# task.report retry contract

Params and result remain unchanged: `{task_id?, pane_id?, status, summary, artifacts?, evidence?}` returns `{task}`. Explicit task ID takes precedence over current pane ownership. Removed panes cannot resolve their former task; use the task ID for later retries.

An identical accepted result succeeds with the current stored task, preserving original `result.reported_at` and current `updated_at`, without events or a new persistence request. Changed status, summary, artifact values/order, or evidence returns `BAD_PARAMS`. A terminal task without an accepted result remains `BAD_PARAMS`; an unknown task remains `NO_SUCH_TASK`. Mistyped params and invalid summary/artifact limits retain existing errors.

Optional-field defaults are applied before comparison. `reported_at` is assigned by the server. The existing Store write lock serializes concurrent reports. An acknowledgment confirms stored state in the running process; it is not a synchronous disk-flush or coordinator-delivery guarantee.

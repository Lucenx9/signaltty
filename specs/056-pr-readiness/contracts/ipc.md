# IPC contract: Verified PR readiness

Existing parameters and method names remain unchanged.

`task.get`, `task.list`, and `task.pr_refresh` return task objects with an additive `stage` field: working, needs_you, in_review, ready_to_merge, or done. It is computed at response time and is absent from saved Task records. Clients must reevaluate or reread time-sensitive projections.

`task.pr_refresh` adds `errors: [{task_id, code, message}]` for batch calls. Tasks whose observation fails in this call are excluded from its tasks list, but their stored PR retains previous facts and records refresh_error and refresh_attempted_at. Single calls still return their typed error after invalidating readiness. An empty batch returns empty tasks and errors arrays.

Task.pr carries the additive fields in [data-model.md](../data-model.md). Unknown or malformed positive evidence never establishes readiness. Error responses retain existing codes and details.

Successful observation IDs are resolved to current task facts at response time. A newer concurrent failure appears in that task view and cannot return an older ready stage. Results for PR URLs replaced during the batch are omitted.

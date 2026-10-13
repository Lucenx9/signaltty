# Report identity and transitions

No new serialized fields. `TaskResult` content consists of status, exact summary, ordered artifacts, and optional evidence. Its `reported_at` belongs to the first acceptance and is excluded from retry matching. Equality uses decoded existing types, including serde_json value equality.

Existing limits remain: summary 1 byte … 8 KiB; at most 32 artifacts. Validation occurs before retry matching. A terminal task only accepts an identical result when its state equals that result's target state. Different content, canceled tasks, and terminal tasks without a result are refused.

The first report changes lifecycle and result and emits `task.result` plus `task.updated`. A retry changes no field and emits neither event. Current review disposition and timestamps survive. Snapshot restore requires no migration.

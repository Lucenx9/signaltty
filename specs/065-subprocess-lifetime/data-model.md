# Runtime data

Command execution owns a child and its process group until success or cleanup. Capture limits are optional per-stream byte counts. Failure distinguishes spawn, output read, timeout, stdout overflow and stderr overflow so callers preserve existing classifications. Nothing is serialized or persisted.

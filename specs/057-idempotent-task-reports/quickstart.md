# Verify accepted report retries

Use the prepared Linux development environment from [docs/17](../../docs/17-agent-development.md).

1. Run `cargo test -p signaltty-core task_report` for content matching and terminal immutability.
2. Run `cargo test -p signaltty-server --test orchestrator_tasks task_report` for IPC retries, concurrent submissions, event counts, refusal, and snapshot restoration.
3. Run `scripts/verify.sh full` for the delivery gate. Its evidence directory records the tested revision and all step outcomes.

The server tests own temporary repositories, sockets, state, and worker processes. They do not use the application's default socket or a paid coding provider.

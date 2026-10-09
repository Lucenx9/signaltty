# Tasks

- [x] Clarify scope, D-Bus/server test seams and implementation plan.
- [x] Red/green: private-bus shutdown saves live resume metadata before releasing inhibitor.
- [x] Red/green: false/initial shutdown, denied/unavailable/restarted service and monitor cleanup.
- [x] ADR-0028, persistence docs and verification feature map.
- [x] Independent Standards and Spec review; address findings.
- [x] Full verification and Rust 1.92 build.
- [x] Publish and link PR; record evidence and proof limits.

## Evidence

`cargo test -p signaltty-server --test logind`: 8 passed.
`scripts/verify.sh full`: passed (`/tmp/signaltty-verify-032/summary.json`), including fmt, clippy warning policy, architecture, workspace tests, server QA, 28 ignored GTK tests and the refresh benchmark.
`cargo +1.92.0 check --workspace --all-targets --locked`: passed.
Spec review: pass, no acceptance gaps.

Proof limits: the tests use a private dbus-daemon and never shut down the host. Power loss and SIGKILL can still skip the save. No desktop AT-SPI run; this change has no GUI surface.

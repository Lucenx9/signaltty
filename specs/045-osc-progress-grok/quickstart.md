# Validation

Run `cargo test -p signaltty-term osc::tests`, `cargo test -p signaltty-agent`,
and `cargo test -p signaltty-server --test screen_detection`.
Real PTY fixtures emit progress, release transitions through input/file barriers,
and inspect wait/get/explain without provider authentication. Launchers discover
Grok and its aliases through bundled_programs. Run `scripts/verify.sh full`
before opening the PR. Live Grok authentication is not required by these fixtures;
actual CLI-version validation must be reported separately.

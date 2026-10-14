# Validation

Use `cargo test -p signaltty-core gui_preference` and `cargo test -p signaltty-agent` for preference/command contracts. Use the server integration tests for actual resume and default worker command behavior. Use the isolated GUI automatic-permissions test for switch persistence and light/dark rendering. Finish with `scripts/verify.sh full`.

After installing the updated app on the other PC, open Preferences and enable Run agents without confirmations once, then start a new supported agent workspace. Existing processes retain their policy. Custom commands remain under user control. Authentication, hook trust and managed provider restrictions remain native.

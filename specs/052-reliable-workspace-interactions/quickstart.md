# Validation

Run `scripts/verify.sh doctor`, targeted GUI tests in separate `GTK_A11Y=none xvfb-run -a dbus-run-session` processes, then `scripts/verify.sh full --output /tmp/reliable-workspace-interactions-final`. The runner records all new GTK cases and captures. Inspect light/dark confirmations, creation validation/pending/retry and 360px/enlarged controls.

Manually on an isolated server: cancel a live terminal close; confirm the original tab after switching selection; close an exited terminal; enter malformed command, correct it, submit; fail creation/startup and retry without duplicate workspace; at 360px use sidebar Board and Changes. Screen-reader, OSK/IME and chooser checks require a real desktop and are reported not run unless performed.

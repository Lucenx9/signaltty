# Validate

Run cargo test -p signaltty-gui sidebar::tests for content regressions.
Run scripts/verify.sh full; inspect emitted shell/workspace-panel light/dark,
large-text and narrow renders. Populate real PTYs using isolated server/socket,
Xvfb and D-Bus to inspect the whole shell. Never use the user's default socket.
Review selection, branch/directory symbols, full title tooltips, metadata hiding
for children and urgency/focus. No real-desktop AT-SPI, OSK or IME claim follows
from isolated display proof.

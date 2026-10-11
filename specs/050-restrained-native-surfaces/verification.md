# Verification

Pending full gate at `/tmp/restrained-native-verification`.
Existing isolated preferences, palette/Git, shell/New Workspace and narrow-board
fixtures passed after the first visual pass. Latest native renders are reviewed
before delivery. Styling does not add behavior, so existing acceptance tests
supply regression coverage; no redundant class-mirroring tests were added.

Before/after evidence compares feature049 native scenes with the restrained
surface scenes. Changes/Worktrees/palette baseline now loads production CSS.

Not run: real desktop screen reader, IME, on-screen keyboard, folder chooser,
notification clicks and paid-provider sessions. Class-based high contrast on
Xvfb is not desktop accessibility certification.

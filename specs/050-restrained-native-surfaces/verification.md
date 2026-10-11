# Verification

`scripts/verify.sh full --output /tmp/restrained-native-verification-final` passed at clean `8bf1d3e57ab15bb5e833bfc9bdc77799e938d3ac`: 47 steps, including 35 isolated GTK tests, workspace tests, server QA, architecture/warning policy and refresh benchmark. Durable summary and 18 selected native renders: [evidence](../../docs/qa/restrained-native-surfaces-2026-10-11/README.md).

All six application-owned surfaces were inspected in light/dark. Preferences and Board also preserve boundaries and navigation under narrow, enlarged-text high contrast. Palette keyboard selection and native focus checks passed. Existing identity, navigation, filtering and persistence tests supply behavior coverage; no redundant class-mirroring tests were added for styling.

| Before | After |
| --- | --- |
| Raised preference cards and double selection ring | Flat previews with single selection boundary and separate focus outline |
| Filled secondary actions, broad rounding | Outline actions, 6px controls/cards and 10px dialog corners |
| Filled palette selection, raised lists | Neutral fill, leading accent marker and flat groups |
| Heavy details elevation | Lighter popover with readable semantic metadata |

Before means feature049 styled native evidence, not older unstyled fixture renders. Details capture originally showed only the window; commit `62a4d9d` generalizes the snapshot helper and captures the popover itself. Both corrected appearances were independently re-reviewed.

Independent Standards and Spec reviewers: zero remaining source or visual findings. Their sole evidence gap (Details capture) is closed. Application behavior is unchanged by the capture fix. The final gate ran after that fix; later commits contain proof/docs only.

Not run: real desktop screen reader, IME, on-screen keyboard, folder chooser, notification clicks and paid-provider sessions. Class-based high contrast on Xvfb is not desktop accessibility certification. New Workspace fixture background Changes error is a placeholder IPC response, not a dialog regression claim.

# Sidebar app identity evidence, 2026-10-14

Original baseline `f8c3f18`, parent PR #106. Original branding source `0377a86`; final integrated source `ec3b4d79457cde524de8b5c1c008e1d9dc9331a4` includes parent fix 29e9aa6. Branding composition/CSS/SVGs remain identical across that integration.

Before/after app captures use the same ten-workspace native fixture and seeded terminal content. The focused native test captures regular app icons at 16/32/64/128px and symbolic icons at 16/24/32px with filesystem theme paths empty. It checks resource provenance, static decorative accessibility/focus, full app-name tooltip, bounds at 200px with Sans 18 and RTL, and both GtkWidget keyboard activation and click opening New Workspace. `native-high-contrast-large-light.png` comes from ADW_DEBUG_HIGH_CONTRAST=1 with StyleManager state asserted.

The desktop-install test uses a disposable prefix with a stub cargo build, checks existing desktop IDs/Exec quoting and compares both installed SVG files byte-for-byte with the source artwork. The full workspace build verifies actual compiled GResources; this stub installation probe does not claim a release build or a real desktop launcher/switcher render.

`red-identity.log` is the staged regression failure against the old static title. `native-high-contrast.log`, `populated-shell.log` and `desktop-install.log` retain focused passing checks. `reviews.md` records both independent axes and final integration review. Remaining manual checks are not inferred from screenshots.

Actual desktop launcher/switcher presentation, screen-reader speech/order, OSK, IME, actual compositor dragging, folder choosers, notification clicks and paid provider sessions were not run. Xvfb accessible GTK names/focus, high contrast and geometry do not certify those paths. Native resize scenes include transient allocation warnings, not failed acceptance assertions.

The initial full run was explicitly superseded when the parent PR advanced. The release report is the completed gate on the final integrated source; serial Rust scheduling preserves every check and timeout on this contended host.

The first integrated gate failed the pre-existing server backpressure test's expected cross-client write order. The actual byte array began with FOLLOW_UP rather than the large x payload. No server or test ordering code changed in this PR. Both isolated cargo invocation and the exact failed workspace-test executable passed afterward on unchanged source. `server-order-failure.txt` shortens the large byte arrays explicitly; `superseded-failed-gate.json` retains that unsuccessful report, and `server-order-exact-recheck.log` retains the exact-binary pass. The final full retry reruns every check without skipping or changing deadlines.

## Completed release gate

`RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/sidebar-identity-full-retry-20261014` passed all 55 steps, including all 43 isolated GTK tests and refresh benchmark, on the clean integrated source above. `verification.json` retains every command, outcome, duration, versions and source identity. Final standard screenshots and `native-identity.log` come from that run; native high-contrast captures precede integration, whose branding code and assets are identical. Selected complete logs are retained here; other successful runner logs remain in the local evidence directory.

## Final test-only parent update

After the full gate, parent PR #106 advanced to f7de184 with a test-only allocation guard in visible_workspace_tools. Integrated as 720b321. Production code, SVG assets and install helper are identical to gated source ec3b4d7; only app_tests.rs changed. The exact updated native test, cargo fmt and GUI all-target Clippy passed. `parent-focus-test-recheck.log` and `final-gui-clippy.log` retain those checks. The full report honestly remains tied to ec3b4d7.

## Final merge preparation

PR #106 was squash-merged as `51757f9`. Integration `5c20759` preserves its final focus-test preparation and narrow-window allocation check, removing the superseded allocation guard. The complete local gate passed again on that integration: 55 steps, all 43 isolated GTK tests and refresh benchmark. Evidence remains in `/home/simone/code/signaltty-pr106-build/verification-pr108/summary.json`. The focus regression also passed with Ubuntu 26.04 GTK in an isolated runtime container. Independent Standards and Spec reviews found no actionable defects; native light/dark images were inspected.

PR #107 was subsequently merged into main as `c7c8a02` and integrated without conflicts. This adds its separately reviewed and CI-passing report retry behavior; sidebar production sources and tests remain identical to `5c20759`. The final PR CI verifies the combined source. Existing manual proof limits remain unchanged.

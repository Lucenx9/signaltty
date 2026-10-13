# Verification

Final source `ec3b4d79457cde524de8b5c1c008e1d9dc9331a4`, parent `29e9aa6`. Full gate passed all 55 steps and all 43 isolated GTK tests plus refresh benchmark on a clean checkout: `RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/sidebar-identity-full-retry-20261014`. All checks and original deadlines retained; serial Rust scheduling used on the contended host.

Native light/dark, 200px/enlarged/RTL and asserted native high-contrast branding scenes were inspected. Resource-only regular icons at 16/32/64/128 and symbolic icons at 16/24/32 render the matching original silhouette. Tests verify static decorative role/focus, app-name tooltip, bounded creation target and actual GTK keyboard activation/click opening New Workspace. Disposable desktop installation verifies both deployed SVGs byte-for-byte with source, with a stub build; the full workspace build compiles actual embedded resources.

Standards and Spec reviewers pinned `29e9aa6...ec3b4d7` and reported no actionable findings. Parent compact-tool fix and both verification-map paragraphs survived integration; branding implementation remained unchanged from 0377a86.

The first full run was superseded when the parent changed. The first integrated run failed a pre-existing server cross-client write-order test; isolated cargo and exact failed binary reruns passed unchanged, then the final full retry passed. [Evidence](../../docs/qa/sidebar-identity-2026-10-14/README.md) retains the shortened failure, failed report and exact-binary pass along with release report, native captures and [reviews](../../docs/qa/sidebar-identity-2026-10-14/reviews.md). No server or ordering-test behavior changed or was skipped.

Actual desktop launcher/switcher presentation, screen-reader speech/order, OSK, IME, compositor dragging, folder chooser, notification clicks and paid provider sessions were not run. Native roles/focus, Xvfb geometry and screenshots do not certify those manual paths.

## Final test-only parent update

After the full gate, parent PR #106 advanced to f7de184 with a test-only allocation guard in visible_workspace_tools. Integrated as 720b321. Production code, SVG assets and install helper are identical to gated source ec3b4d7; only app_tests.rs changed. The exact updated native test, cargo fmt and GUI all-target Clippy passed. `parent-focus-test-recheck.log` and `final-gui-clippy.log` retain those checks. The full report honestly remains tied to ec3b4d7.

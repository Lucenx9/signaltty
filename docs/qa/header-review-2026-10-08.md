# Native header review — 2026-10-08

## Scope and diagnosis
PR39 was merged after rust, minimum-rust and CodeRabbit checks passed, at `4815c16678dad0128053bde775a06b9691592da5`. This next PR addresses the shell-header wayfinding finding from [the broad UI review](ui-review-2026-10-08.md). It preserves the existing agent summary and uses existing branch/directory context for shells.

The actual App/snapshot GTK regression failed in 0.64s: context was `""` instead of `feature/header · /tmp/signaltty <literal>&`. The native baseline also shows the dangling `/`. Ranked hypotheses were (1) assigning only agent summary, (2) separator visibility bound to monogram, (3) allocation hiding valid text. The empty string ruled out (3); source and the native fix confirmed (1) and (2). No debug instrumentation was added.

Reproduction command:
```sh
GTK_A11Y=none SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-header-repro xvfb-run -a -s '-screen 0 1600x1200x24' dbus-run-session -- cargo test -p signaltty-gui workspace_header_context_survives_shell_agent_and_empty_transitions -- --ignored --test-threads=1
```

## Visual assessment

| Before | After | Why |
|---|---|---|
| Shell workspace shows name and dangling `/` | Branch · path, or path without branch | Makes workspace location visible without hover at ordinary widths |
| Separator follows workspace monogram | Separator follows nonempty context visibility | No dangling punctuation at startup, empty context or last-workspace removal |
| Agent summary | Same agent summary and full location tooltip | Keeps the existing attention-first agent workflow |
| Long header labels compete with fixed controls | Existing native ellipsizing retains fixed controls inside 360px | Preserves New Tab, menu and sidebar access |

Apply apple-design wayfinding/feedback and emil-design-eng hierarchy/restraint through existing native controls. No CSS, custom font, animation, dependency, IPC or session lifecycle change.

## Review disposition
Sonnet5.5 through Claude Code/T3 found no code blockers. Nonblocking observations: extreme 360px/Sans18 labels reduce to ellipses; tooltip assertions were partial; setting restoration is success-path only; additional light shell proof and durable artifacts were pending. Light shell capture and durable evidence are supplied by the final gate. The runner isolates every display test into its own process, so a panic cannot leak settings to the next test. Extreme label truncation is an explicit limit below. Grok4.7 direct xAI found no functional blockers. Its tooltip, home abbreviation, ellipsis, explicit sidebar-toggle identification and hidden mark suggestions are covered by additional assertions. The attention-pill case remains a follow-up candidate. The inconsistent docs06 breadcrumb description is corrected. Sonnet round 2 also found no code blockers; the supported reproduction exports evidence so its capture helper pumps the main loop before layout assertions. The full runner enables that same capture path. Full gate passed.

## Proof limits
At 360px with Sans18 and a very long name/path, both labels can reduce to ellipses. Controls remain visible, but this does not prove readable location without hover at extreme widths. Full context is a pointer tooltip; screen-reader, keyboard tooltip, actual desktop high contrast and IME behavior are not certified. A native Adwaita minimum-size warning of 356–362px can appear while the requested window is 360px (350px inside CSD with Cairo) and inspected controls stay inside it; this PR does not redesign the compact header. The test uses the production App and rendering with fixture snapshots, not a paid/live provider session. The pre-fix baseline initially omitted production CSS/icons; it establishes the empty context and dangling punctuation, not a theme comparison. Final captures load production resources and default Signal theme.

Board refresh, board narrow attention visibility and measured contrast work remain candidates from the earlier review.

## Verification environment
The first full run stopped at the new test because it assumed the requested 360px window width equals the inner allocation. Cairo/X11 exposes 350px inside client decorations. The test now waits for a positive allocation at or below 360px and checks each control against that actual width, rather than a hard-coded upper bound. No application sizing or existing assertion is weakened.

## Completed verification
`scripts/verify.sh full` passed in `target/verification/full-umnjk34_`: 37 steps, 25 isolated GTK display tests, workspace build/tests, architecture/fmt/Clippy warning policy, real server QA and refresh benchmark. [Summary](2026-10-08-header-quality/verification-summary.json) records base `4815c16` with the working patch; [source hashes](2026-10-08-header-quality/verified-sources.json) identify the exact verified Rust files. No code changed after this run.

Inspected final production-resource native captures: [shell light](2026-10-08-header-quality/header-shell-light.png), [shell dark](2026-10-08-header-quality/header-shell-dark.png), [agent summary](2026-10-08-header-quality/header-agent-dark.png), [narrow light](2026-10-08-header-quality/header-narrow-light.png), [narrow dark large](2026-10-08-header-quality/header-narrow-dark-large.png). [Baseline](2026-10-08-header-quality/header-before.png) proves the missing context with the resource limitation described above.

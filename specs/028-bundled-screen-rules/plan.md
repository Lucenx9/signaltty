# Plan

Extend `ScreenRule` with flat `all`/`not` regex lists (herdr's nested gates
flatten into several rules or regex alternations for these three agents).
Bundled manifests are TOML files in `crates/signaltty-agent/screen/`, embedded
with `include_str!` and parsed through `parse_manifest`, so they obey the same
validation. They are not overlays: overlays keep owning hook overrides, and
`Ctx::screen_rules` picks user rules first, bundled rules otherwise.

Test-first: engine all/not unit tests, bundled fixture tests, Ctx selection test,
then a real-PTY `pi` test. Record bundling + override in ADR-0025 and docs/07.
No Gemini this round; a Claude review agent reviews the diff before the PR.

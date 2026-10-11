# Third-party materials

Signaltty's own contributions are licensed under MIT OR Apache-2.0, as declared
in `Cargo.toml`; see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). This choice does not replace the licenses or
notices of third-party material. Upstream contributors retain their rights.

Cargo describes complete source packages: `signaltty-agent` declares
`(MIT OR Apache-2.0) AND Apache-2.0` for its bundled Herdr material, and
`signaltty-gui` declares `(MIT OR Apache-2.0) AND MIT` for its T3 Code
adaptations. The built application additionally incorporates the agent package;
AppStream therefore declares `(MIT OR Apache-2.0) AND Apache-2.0 AND MIT`.
The `AND` clauses preserve the incorporated terms regardless of which license
is chosen for Signaltty's own contributions.

## Herdr detection rules — Apache-2.0

The 13 TOML files in `crates/signaltty-agent/screen/` are adaptations of
`src/detect/manifests/` from [herdrdev/herdr](https://github.com/herdrdev/herdr/tree/2563803dca97c040beaf3dc3acdcb5a3221b4238),
revision `2563803dca97c040beaf3dc3acdcb5a3221b4238`:
`antigravity`, `claude`, `codex`, `copilot` (upstream `github-copilot`),
`cursor`, `droid`, `gemini`, `grok`, `kilo`, `muse`, `opencode`, `pi`, and
`qodercli`.

The file headers retain the existing attribution to Herdr contributors and
describe the changes to rule syntax, regions, binary scope and idle fallback.
Those headers must stay with the files. The full upstream Apache-2.0 license
is reproduced in [LICENSE-APACHE](LICENSE-APACHE). No project-level `NOTICE`
was present at this revision; the Windows ConPTY notice does not concern
these manifests.

`crates/signaltty-agent/src/screen.rs` implements Herdr-style screen regions;
`src/bundled.rs` loads the adapted rules and tests their behavior. These are
included in the provenance review scope; the confirmed ports are the TOML
files, not an assertion that all this Rust code was copied.

## T3 Code visual references — MIT

The GTK styles and appearance previews adapt visual patterns from
[pingdotgg/t3code](https://github.com/pingdotgg/t3code/tree/98beed1a226c42b85c52ff5ee9d5dbb67d776a77),
including status pulses, dialog treatments, theme families and preview orbs.
Documented references include `apps/web/src/index.css`,
`packages/shared/src/themePalettes.ts`, and thread/status components.
See `specs/017-themes/research.md`, `specs/039-t3code-polish/spec.md`,
`crates/signaltty-gui/data/style.css`, and appearance preview code in
`crates/signaltty-gui/src/preferences.rs`.

Copyright (c) 2026 T3 Tools Inc.

The complete upstream notice and MIT permission are preserved in
[licenses/t3code-MIT.txt](licenses/t3code-MIT.txt). This attribution covers
the documented adaptations without claiming that Signaltty's entire GUI is
copied from T3 Code or that its other dependencies share this license.

## GitHub Spec Kit — MIT

`.specify/templates/`, `.specify/scripts/bash/`, and `.specify/commands/` are
vendored from GitHub Spec Kit at
`8d3f64cdccc877b6a297bc8167131103bb5b8ca0`. See
[.specify/SOURCES.md](.specify/SOURCES.md) for provenance and refresh rules.

Copyright GitHub, Inc.

The complete upstream MIT notice is in [.specify/LICENSE](.specify/LICENSE).
Keep it with these sources and with copies of the generated commands.

## Distribution and dependencies

Include this document, `LICENSE-MIT`, `LICENSE-APACHE`, and
`licenses/t3code-MIT.txt` with binary distributions, because adapted screen
rules and GUI resources are compiled into the application. Source
distributions must also retain `.specify/LICENSE` and file-level notices.

The agent and GUI crate directories each carry local `LICENSE-MIT`,
`LICENSE-APACHE`, and `NOTICE` files so Cargo source archives include them.
The GUI archive additionally carries `licenses/t3code-MIT.txt`. These are real
files inside the packages, not links to files outside the package roots.
`scripts/tests/test_license_packaging.py` checks their presence with
`cargo package --list` and compares copied license texts with their canonical
workspace versions. Keep both versions in sync when a license text changes.

This is an inventory of known directly incorporated material, not a complete
notice bundle for all dependencies. Rust dependencies and native libraries
retain their own licenses. Before distributing binaries, inventory the actual
build and package, preserve the relevant dependency notices, and meet any
native-library redistribution requirements. GTK's native LGPL license is
separate from the Rust bindings' license.

The external skills installed by `scripts/setup-agent.sh` are listed in
[docs/15-agent-skills.md](docs/15-agent-skills.md). The installer copies skill
directories; it does not currently ensure that repository-level upstream
license files accompany every installed skill. Redistributing those installed
skills requires a separate notice review.

cmux is a documented product/source reference, not a confirmed incorporated
component in this review. Its inspected revision has GPL and BUSL scopes;
do not import its implementation under Signaltty's MIT/Apache declaration.
See the [provenance review](docs/research/2026-10-11-license-provenance.md)
for evidence and unresolved checks.

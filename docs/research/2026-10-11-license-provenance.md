# License provenance review — 2026-10-11

Baseline: Signaltty `ab9d849`, clean working tree before this review. This is a
technical provenance review, not a legal opinion or a complete clearance of
the repository or earlier releases.

## Confirmed incorporation

**Herdr:** all 13 bundled files in `crates/signaltty-agent/screen/` identify
themselves as ports from revision `2563803`. Their headers preserve attribution
and describe modifications. This is actual derivation, not merely a product
reference. History corroborates it, including `de6e873` (Pi/OpenCode/Cursor),
`9a2f2a0` (Claude), `8338b5c` (Codex), and `a0fe2a8` (Muse/Antigravity).
The revision resolves to `2563803dca97c040beaf3dc3acdcb5a3221b4238`.
Its [license](https://github.com/herdrdev/herdr/blob/2563803dca97c040beaf3dc3acdcb5a3221b4238/LICENSE)
and Cargo manifest identify Apache-2.0. The license has a standard placeholder
appendix rather than a named copyright holder. The existing attribution
“Herdr contributors” is descriptive; it does not establish ownership.
The upstream tree has no project-level `NOTICE`. Its Windows ConPTY notice is
unrelated to the imported detector files.

**Spec Kit:** `.specify/SOURCES.md` explicitly records verbatim vendoring of
templates, Bash scripts and command instructions from
`8d3f64cdccc877b6a297bc8167131103bb5b8ca0`. The
[upstream license](https://github.com/github/spec-kit/blob/8d3f64cdccc877b6a297bc8167131103bb5b8ca0/LICENSE)
is MIT, copyright GitHub, Inc. Documentation and templates are covered too.
The declaration of verbatim provenance was inspected; this review did not
recompare every vendored file against upstream.

**T3 Code:** the repository explicitly references styles, theme families,
status indicators, dialogs, diff panels and appearance previews. See
`docs/14-product-direction.md`, `specs/017-themes/research.md`, and
`specs/039-t3code-polish/spec.md`. The last document pins `98beed1a`, resolving
to `98beed1a226c42b85c52ff5ee9d5dbb67d776a77`. Its
[license](https://github.com/pingdotgg/t3code/blob/98beed1a226c42b85c52ff5ee9d5dbb67d776a77/LICENSE)
is MIT, copyright 2026 T3 Tools Inc.
The upstream `index.css` and `themePalettes.ts` were inspected against local
style references and theme research. For example, upstream's status animation
uses a two-second cycle, while the local GTK animation uses 800ms alternation;
the local palette research documents independently calculated colors.
This supports documented visual adaptation, not a finding that the whole GUI
or its Rust implementation was copied verbatim. Preserve the upstream MIT
notice for these adaptations rather than rely on a judgment that individual
tokens or fragments are too small to protect.

## cmux: sensitive license scope, incorporation not established

The existing source research pins cmux
`1a7d5cd08ad677359c0a273c10e13b853b86da31`. Its
[license](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/LICENSE)
states GPL-3.0-or-later for the app/CLI and most other directories; listed
server directories have separate BUSL-1.1 licenses and production-use
restrictions. A source-visible repository is not automatically permissive.

Local research discusses mechanisms such as event replay, hooks and approvals.
No explicit cmux code port was found by the text/history searches. This is
not proof of absence: no exhaustive clone/similarity comparison, translated-code
analysis or review of every historical patch was performed. The fact that cmux
uses Swift and Signaltty uses Rust does not exclude derivation.

Before making a strong no-cmux-code claim, review the introduction commits
for event recovery, provider hook/approval bridges and agent automation against
the pinned source references in `2026-10-03-cmux-herdr-code.md`. If protected
implementation was incorporated, attribution alone is insufficient: assess
the applicable GPL distribution requirements or applicable BUSL grant, obtain
an appropriate license, or replace the material through a documented independent
implementation. Do not describe a cosmetic rename as an independent rewrite.

## Legal distinction and limits

In the EU, the [Software Directive, article 1(2)](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32009L0024)
distinguishes protected program expression from ideas and principles. That
supports implementing the same workflow independently; it does not authorize
copying source, adapted source, text or assets without permission.
A graphical interface can also have copyright protection as an original work
under the ordinary copyright regime; see
[CJEU C-393/09](https://eur-lex.europa.eu/legal-content/EN/TXT/PDF/?uri=celex%3A62009CJ0393).
Patent, trademark, contract, contribution ownership and jurisdiction-specific
questions were not cleared by this technical review.

## Corrections made in this checkout

The baseline declared MIT OR Apache-2.0 in `Cargo.toml` and README but had no
tracked `LICENSE`, `COPYING` or `NOTICE` files. Herdr headers linked to the
Apache license, while Spec Kit provenance named MIT; neither supplied its
full license text locally.

This change adds root MIT and Apache-2.0 texts consistent with the existing
project declaration, the exact pinned Spec Kit MIT notice in `.specify/LICENSE`,
the exact pinned T3 Code MIT notice in `licenses/t3code-MIT.txt`, and
`THIRD_PARTY_NOTICES.md` mapping the material to its provenance and terms.
The Apache text is byte-for-byte the inspected Herdr license. The root MIT
text uses the existing project's contributor attribution; this is not an
ownership audit. README now makes third-party exceptions explicit.
Existing port/change notices remain intact. No runtime implementation was
removed or changed, and no published Git history was rewritten.

## Remaining release work

1. Make release packaging install the notice bundle alongside binaries. The
   manifests and GUI resources are compiled in; a notice available only in
   the source repository does not ensure every binary recipient receives it.
2. Inventory the actual dependency graph and native libraries of each release.
   `cargo metadata --locked --offline --format-version 1` succeeded against
   the reviewed lockfile and reported license expressions for all 258 external
   Rust packages. This includes development/build/target-specific packages,
   not just shipped code. License metadata is a starting inventory, not proof
   of compliance or a complete notice bundle. Native libraries are outside
   that inventory: for example,
   [GTK is LGPL-2.1-or-later](https://docs.gtk.org/gtk4/), independently of the
   Rust bindings. Check actual linking and redistribution rather than change
   the whole project license based on a dependency name.
3. Audit icon/image/font origins. Local SVGs are introduced in Signaltty
   commits without a detected third-party header, but this does not prove
   originality. No exhaustive asset comparison was performed. Recheck any
   downloaded or generated material and the license of bundled assets.
4. Fix upstream-license preservation in the external skill installer before
   redistributing installed skill bundles. It copies individual directories;
   repository-level license files may be left behind. The source list in
   `docs/15-agent-skills.md` is not a substitute for the required notices.
5. Review the targeted cmux introduction patches described above and record
   the result. Escalate actual restrictive-license derivations or disputed
   visual/asset ownership to a qualified lawyer before distribution.

Deleting inspiration links or rewriting Git history is not a compliance fix.
Keep the provenance. The added notices address the identified missing texts
in the current source checkout; they do not retroactively establish compliance
of prior distributed artifacts.

## Verification

`cargo build --workspace` and `scripts/verify.sh fast` passed. Fast-run evidence:
`target/verification/fast-k0x483oe/summary.json`. Local Markdown links and
`git diff --check` passed. The three imported license texts were compared
byte-for-byte with the pinned upstream URLs and matched. Before opening the PR,
`scripts/verify.sh full` also passed all 47 steps, including server QA, 35 isolated
GTK tests and the refresh benchmark; evidence is in
`target/verification/full-bcd_81lz/summary.json`. Representative dark and narrow
light renders were inspected. These checks verify the edit and existing build;
they do not certify legal compliance or complete visual acceptance.

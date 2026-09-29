# 15 — Agent Skills: install once, use every phase

signaltty is built spec-first (`.specify/`, from
[github/spec-kit](https://github.com/github/spec-kit)) with skills doing the
heavy lifting in every phase. This doc is the routing table: which skill to
load, when. The constitution (`.specify/memory/constitution.md`) enforces it.

## Install (any environment)

One command from any checkout, only bash + git required:

```sh
scripts/setup-agent.sh all            # skills + spec-kit commands + git hooks
scripts/setup-agent.sh verify         # re-check this environment
scripts/setup-agent.sh skills list    # the 72 curated skills
```

`all` installs the pinned skill set into every detected harness home
(`~/.agents/skills`, `~/.claude/skills`, `~/.codex/skills`), renders the
spec-kit slash commands (`/speckit-specify`, `/speckit-plan`, …) into the
project-local dir of each detected harness, and points git at `.githooks/`.
Override with `--target DIR` / `--agent NAME` (`auto|all|claude|codex|cursor|
opencode|gemini|copilot|muse`). Spec-kit sources are vendored verbatim under
`.specify/` (see `.specify/SOURCES.md`), so slash commands install offline;
skills are shallow-cloned at pinned revs into
`$XDG_CACHE_HOME/signaltty/upstream` (override `--cache`).

## Phase routing

Load the phase's skills **before** working, not after.

| Phase | Command | Skills |
|---|---|---|
| Discover | read code, `research` | `research` (primary sources → repo Markdown), `how` + `why`, `recall` (context rebuild) |
| Specify | `/speckit-specify`, `/speckit-clarify` | `grill-with-docs` (interview → ADRs + glossary), `to-spec`, `domain-modeling` (terms), `constitution` gates the bar (`docs/14`) |
| Plan | `/speckit-plan` | `architect` (types/signatures first), `domain-modeling`, `codebase-design`, `exhaust-the-design-space`, `wayfinder` (multi-session) |
| Tasks | `/speckit-tasks` | `to-tickets` (tracer bullets + blocking edges), `sequence-verifiable-units` |
| Implement | `/speckit-implement` | `implement` + `tdd` + `karpathy-guidelines`, principles: `fix-root-causes`, `type-system-discipline`, `model-the-domain`, `boundary-discipline`, `laziness-protocol`, `subtract-before-you-add`, `minimize-reader-load`, `build-the-lever`, `make-operations-idempotent`, `migrate-callers-then-delete-legacy-apis`, `separate-before-serializing-shared-state`, `redesign-from-first-principles`, `foundational-thinking`, `outcome-oriented-execution`, `never-block-on-the-human`, `experience-first` |
| Review | `/speckit-analyze`, PR | `code-review`, `interrogate`, `no-comments`, `blast-radius`, `attack-the-premise` |
| Verify | gates green | `prove-it-works`, `create-verification-skill` (+ `maintain-verification-skill`), `test-behavior-not-implementation` |
| UI/Visual | any GUI work | `frontend-design`, `emil-design-eng`, `apple-design`, `animation-vocabulary`, `find-animation-opportunities`, `review-animations`, `improve-animations`, `emilkowalski-prototype` vs `prototype`; bar is `docs/14` §6–7, verified light + dark |
| Debug | red loop | `diagnosing-bugs`, `figure-it-out`, `resolving-merge-conflicts`, `pstack-tdd`/`pstack-teach` |
| Parallel | fan-out | `arena` (competing candidates), `swarm` (workers → one report), `guard-the-context-window`, `show-me-your-work` |
| Docs | write/edit | `writing-for-agents` (AGENTS.md/skills), `technical-writing`, `doc` rows in `docs/` + ADR |
| Meta | improve the system | `skill-creator`, `reflect`, `improve-codebase-architecture`, `encode-lessons-in-structure`, `handoff` |
| Router | unsure | `ask-matt`; plain-language check: `bro` |

## Sources and curation

| Source | Rev | Skills | License |
|---|---|---|---|
| [mattpocock/skills](https://github.com/mattpocock/skills) | `c55ee46` | 20 engineering/productivity | MIT |
| [anthropics/skills](https://github.com/anthropics/skills) | `8a1541c` | `frontend-design`, `skill-creator` | Apache-2.0 |
| [emilkowalski/skills](https://github.com/emilkowalski/skills) | `d16ebe6` | 7 design/motion (`prototype` → `emilkowalski-prototype`) | MIT |
| [multica-ai/andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills) | `2c60614` | `karpathy-guidelines` | MIT |
| [cursor/plugins](https://github.com/cursor/plugins) (`pstack/`) | `adf3218` | 19 workflows + 23 `principle-*` (`tdd` → `pstack-tdd`, `teach` → `pstack-teach`) | MIT |
| [github/spec-kit](https://github.com/github/spec-kit) | `8d3f64c` | vendored `.specify/` + 10 slash commands | MIT |

Excluded deliberately: harness-specific setup skills, TS-only skills,
writing-for-publication skills, and anthropics source-available doc skills
(`docx`/`pdf`/`pptx`/`xlsx`). Rename-on-collision keeps both variants
addressable (see `skills list`).

Refresh: `scripts/setup-agent.sh pins` shows pinned vs latest; to move, update
the `PIN_*` vars (and `.specify/SOURCES.md` for spec-kit), re-run `all`,
commit. Never hand-edit an installed skill: fix upstream or drop it.

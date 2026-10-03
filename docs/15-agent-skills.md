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

`skills check` compares installed contents with the cached pinned revision, not
just file presence. With a custom cache pass `skills check --cache DIR` explicitly.
If the cache is unavailable, verification fails and asks for installation again;
the check does not fetch or modify installed skills.

## Task routing

Load the workflow for the current task, then read additional references only when
that workflow needs them. Already clarified requirements and existing test seams
remain valid across phases; do not repeat interviews or publish issues just because
a generic skill includes those steps.

| Task | Load first | Add when needed |
|---|---|---|
| Investigate a question | `research` or `how` | `why` for an architectural decision |
| Specify a feature | `/speckit-specify`, `/speckit-clarify` | `domain-modeling` for new concepts; interview only unresolved requirements |
| Design a substantial change | `/speckit-plan`, `architect` | `codebase-design` for module boundaries |
| Break down work | `/speckit-tasks` | `to-tickets` only when publishing tickets is requested |
| Implement | `implement`, `tdd`, `karpathy-guidelines` | A `principle-*` skill addressing the concrete design or failure |
| Review a diff | `code-review` | `interrogate` for adversarial review; `blast-radius` for cross-module risk |
| Verify a change | project `verify-signaltty`, `principle-prove-it-works` | `maintain-verification-skill` when coverage changes |
| Change GTK visuals | `frontend-design`, `emil-design-eng` | Motion/prototype skills only for those changes; apply `docs/17` visual rubric |
| Diagnose a failure | `diagnosing-bugs` | `figure-it-out` when no narrower workflow applies |
| Write documentation | `technical-writing` | `writing-for-agents` for instructions or skills |

The canonical project verification skill is
[`.agents/skills/verify-signaltty`](../.agents/skills/verify-signaltty/SKILL.md).
Claude, Cursor and Copilot have relative links to the same source; other harnesses
can follow the pointer in `AGENTS.md`. It is repository-owned and is not replaced
by the upstream skill installer. `scripts/verify.sh fast` checks those links.

See [agent development](17-agent-development.md) for environment setup, worktree
isolation, handoffs and the task bank used to evaluate changes to this routing.

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

# .specify/ provenance

`templates/`, `scripts/bash/`, and `commands/` are vendored verbatim from
[github/spec-kit](https://github.com/github/spec-kit) at rev
`8d3f64cdccc877b6a297bc8167131103bb5b8ca0` (2026-09-29, MIT). Do not edit them
in place: adaptations live in `memory/constitution.md`, which every command
reads before running.

Refresh procedure: shallow-clone spec-kit, `diff -r` against these dirs, copy
verbatim, update the rev above and the pin in `scripts/setup-agent.sh`, then run
`scripts/setup-agent.sh commands install --agent <each>` to re-render slash
commands.

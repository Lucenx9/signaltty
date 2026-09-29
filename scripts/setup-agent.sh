#!/usr/bin/env bash
# setup-agent.sh — one-command agent automation setup for signaltty.
# Works from any checkout on any machine with only bash + git:
#   scripts/setup-agent.sh all
# See docs/15-agent-skills.md for the full routing reference.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Pinned upstream revs (refresh: `setup-agent.sh pins`, then edit + commit).
PIN_mattpocock="c55ee46073ed923f86ce59a5eb3b6d895095d1b7"
PIN_anthropics="8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4"
PIN_emilkowalski="d16ebe60d09a5ba2afcb7054ede9d0a10c9f6128"
PIN_karpathy="2c606141936f1eeef17fa3043a72095b4765b9c2"
PIN_pstack="adf3218ca2f5b9971eedc07a76bef22df7701539"
PIN_speckit="8d3f64cdccc877b6a297bc8167131103bb5b8ca0" # vendored under .specify/, see SOURCES.md

URL_mattpocock="https://github.com/mattpocock/skills"
URL_anthropics="https://github.com/anthropics/skills"
URL_emilkowalski="https://github.com/emilkowalski/skills"
URL_karpathy="https://github.com/multica-ai/andrej-karpathy-skills" # fork of forrestchang/andrej-karpathy-skills
URL_pstack="https://github.com/cursor/plugins"
URL_speckit="https://github.com/github/spec-kit"

SPARSE_mattpocock="skills"
SPARSE_anthropics="skills"
SPARSE_emilkowalski="skills"
SPARSE_karpathy="skills"
SPARSE_pstack="pstack/skills"

CACHE_DEFAULT="${XDG_CACHE_HOME:-$HOME/.cache}/signaltty/upstream"

# Curated skill table: repo|source path|installed name|one-liner.
# Installed names ending in "-tdd/-teach/prototype" are renamed on collision.
read -r -d '' SKILLS <<'EOF' || true
mattpocock|skills/engineering/ask-matt|ask-matt|Router: which skill or flow fits the situation.
mattpocock|skills/engineering/code-review|code-review|Review changes since a fixed point on Standards plus Spec axes.
mattpocock|skills/engineering/codebase-design|codebase-design|Shared vocabulary for designing deep modules.
mattpocock|skills/engineering/diagnosing-bugs|diagnosing-bugs|Diagnosis loop for hard bugs and perf regressions.
mattpocock|skills/engineering/domain-modeling|domain-modeling|Build and sharpen the domain model: terminology, CONTEXT, ADRs.
mattpocock|skills/engineering/grill-with-docs|grill-with-docs|Relentless interview to sharpen a plan; writes ADRs plus glossary.
mattpocock|skills/engineering/implement|implement|Implement a piece of work from a spec or ticket set.
mattpocock|skills/engineering/improve-codebase-architecture|improve-codebase-architecture|Scan for deepening opportunities; visual report, then grill.
mattpocock|skills/engineering/prototype|prototype|Throwaway code prototype to answer a design question.
mattpocock|skills/engineering/research|research|Investigate against high-trust primary sources; capture as repo Markdown.
mattpocock|skills/engineering/resolving-merge-conflicts|resolving-merge-conflicts|Resolve an in-progress git merge or rebase conflict.
mattpocock|skills/engineering/tdd|tdd|Test-driven development, red-green-refactor, integration tests.
mattpocock|skills/engineering/to-spec|to-spec|Turn the current conversation into a spec.
mattpocock|skills/engineering/to-tickets|to-tickets|Break a plan or spec into tracer-bullet tickets with blocking edges.
mattpocock|skills/engineering/wayfinder|wayfinder|Plan huge multi-session work as a map of decision tickets.
mattpocock|skills/engineering/wizard|wizard|Interactive bash wizard for human-only steps (infra, secrets).
mattpocock|skills/productivity/grill-me|grill-me|Relentless interview to sharpen a plan or design.
mattpocock|skills/productivity/handoff|handoff|Compact a conversation into a handoff doc for another agent.
mattpocock|skills/productivity/teach|teach|Teach the user a skill or concept within this workspace.
mattpocock|skills/productivity/writing-for-agents|writing-for-agents|Write or edit agent docs: skills, AGENTS.md, CLAUDE.md.
anthropics|skills/frontend-design|frontend-design|Aesthetic direction for distinctive, non-templated UI.
anthropics|skills/skill-creator|skill-creator|Create, edit, and evaluate skills.
emilkowalski|skills/emil-design-eng|emil-design-eng|UI polish, component design, and animation philosophy.
emilkowalski|skills/apple-design|apple-design|Apple interface-design and fluid-motion principles.
emilkowalski|skills/animation-vocabulary|animation-vocabulary|Vague motion description to exact term, reverse-lookup glossary.
emilkowalski|skills/find-animation-opportunities|find-animation-opportunities|Read-only: UI that should animate (exact values) and what should not.
emilkowalski|skills/improve-animations|improve-animations|Read-only codebase motion audit with prioritized plans.
emilkowalski|skills/review-animations|review-animations|Strict review of animation code; approval is earned.
emilkowalski|skills/prototype|emilkowalski-prototype|Genuinely different UI versions behind a visual picker.
karpathy|skills/karpathy-guidelines|karpathy-guidelines|Think before coding, simplicity first, surgical changes.
pstack|pstack/skills/architect|architect|Sketch types, signatures, module structure before code.
pstack|pstack/skills/arena|arena|Spawn N parallel candidates, pick a base, graft the best parts.
pstack|pstack/skills/blast-radius|blast-radius|Find what a change could break beyond the diff; prove safety by running code.
pstack|pstack/skills/bro|bro|Restate the last message in plain human language, no jargon.
pstack|pstack/skills/create-verification-skill|create-verification-skill|Generate a project-local verification skill driving the app like a user.
pstack|pstack/skills/figure-it-out|figure-it-out|Auditable playbook for large migrations; hypothesis loop plus decision log.
pstack|pstack/skills/how|how|Code walkthroughs: subsystem architecture, runtime flow, ownership.
pstack|pstack/skills/interrogate|interrogate|Multi-reviewer adversarial review from independent angles.
pstack|pstack/skills/maintain-verification-skill|maintain-verification-skill|Periodic pass keeping the verification skill and feature map honest.
pstack|pstack/skills/no-comments|no-comments|Spawn Comment Sicko review, fix accepted findings.
pstack|pstack/skills/recall|recall|Reconstruct working context from history and live state into a brief.
pstack|pstack/skills/reflect|reflect|Three parallel transcript reviews into concrete edits on existing skills.
pstack|pstack/skills/show-me-your-work|show-me-your-work|Keep a reviewable decision trail for long or unattended work.
pstack|pstack/skills/swarm|swarm|Fan out N parallel workers, drain, return one report.
pstack|pstack/skills/tdd|pstack-tdd|TDD only when explicitly asked or the bug has a cheap local test target.
pstack|pstack/skills/teach|pstack-teach|Explain a body of work plainly via how plus why.
pstack|pstack/skills/technical-writing|technical-writing|Layered standard: Diataxis plus Google style plus STE.
pstack|pstack/skills/unslop|unslop|Cut AI tells from any writing; always applies.
pstack|pstack/skills/why|why|Design rationale, regressions, postmortems via parallel evidence queries.
pstack|pstack/skills/principle-attack-the-premise|principle-attack-the-premise|After 2 plus failed fixes sharing one premise, question the premise.
pstack|pstack/skills/principle-boundary-discipline|principle-boundary-discipline|Guards at system boundaries; trust internal types.
pstack|pstack/skills/principle-build-the-lever|principle-build-the-lever|Build the rerunnable tool instead of working by hand.
pstack|pstack/skills/principle-encode-lessons-in-structure|principle-encode-lessons-in-structure|Second occurrence of a rule becomes a lint, flag, or check.
pstack|pstack/skills/principle-exhaust-the-design-space|principle-exhaust-the-design-space|Novel decision: 2 to 3 competing prototypes, compare, then commit.
pstack|pstack/skills/principle-experience-first|principle-experience-first|User delight over implementation convenience.
pstack|pstack/skills/principle-fix-root-causes|principle-fix-root-causes|Reproduce, ask why to the root, fix there.
pstack|pstack/skills/principle-foundational-thinking|principle-foundational-thinking|Data structures and scaffolding order right before logic.
pstack|pstack/skills/principle-guard-the-context-window|principle-guard-the-context-window|Bulk to subagents; summaries, not raw payloads, in main thread.
pstack|pstack/skills/principle-laziness-protocol|principle-laziness-protocol|Delete; smallest change that solves the problem.
pstack|pstack/skills/principle-make-operations-idempotent|principle-make-operations-idempotent|Converge to the same end state across retries and restarts.
pstack|pstack/skills/principle-migrate-callers-then-delete-legacy-apis|principle-migrate-callers-then-delete-legacy-apis|Migrate plus delete the old API in one wave, no compat layers.
pstack|pstack/skills/principle-minimize-reader-load|principle-minimize-reader-load|Collapse one-caller wrappers, shrink mutable scope.
pstack|pstack/skills/principle-model-the-domain|principle-model-the-domain|Encode the domain in a structure, not scattered conditionals.
pstack|pstack/skills/principle-never-block-on-the-human|principle-never-block-on-the-human|Proceed on reversible work; confirm only irreversible actions.
pstack|pstack/skills/principle-outcome-oriented-execution|principle-outcome-oriented-execution|Converge on target architecture; no throwaway compat states.
pstack|pstack/skills/principle-prove-it-works|principle-prove-it-works|Verify against the real artifact before declaring done.
pstack|pstack/skills/principle-redesign-from-first-principles|principle-redesign-from-first-principles|Integrate new requirements as if foundational, not bolted on.
pstack|pstack/skills/principle-separate-before-serializing-shared-state|principle-separate-before-serializing-shared-state|Eliminate concurrent-writer sharing first.
pstack|pstack/skills/principle-sequence-verifiable-units|principle-sequence-verifiable-units|Small units each ending in a verifiable state.
pstack|pstack/skills/principle-subtract-before-you-add|principle-subtract-before-you-add|Remove dead or redundant code first, then build.
pstack|pstack/skills/principle-test-behavior-not-implementation|principle-test-behavior-not-implementation|Call code like users do; literal expected values.
pstack|pstack/skills/principle-type-system-discipline|principle-type-system-discipline|Illegal states unrepresentable; parse at boundaries; exhaust variants.
EOF

usage() {
  cat <<'USAGE'
Usage: setup-agent.sh <area> <action> [options]

  skills install [--target auto|DIR] [--cache DIR] [--dry-run]
  skills check   [--target auto|DIR]
  skills list
  commands install --agent auto|all|claude|codex|cursor|opencode|gemini|copilot|muse [--dry-run]
  commands check   --agent auto|all|claude|codex|cursor|opencode|gemini|copilot|muse
  hooks install
  pins
  verify
  all [--target ...] [--agent ...] [--cache DIR]   # skills + commands + hooks
USAGE
}

die() { echo "setup-agent.sh: $*" >&2; exit 1; }
need_git() { command -v git >/dev/null || die "git is required"; }

pin_of() { # $1=repo key -> pinned rev
  case "$1" in
    mattpocock) echo "$PIN_mattpocock" ;;
    anthropics) echo "$PIN_anthropics" ;;
    emilkowalski) echo "$PIN_emilkowalski" ;;
    karpathy) echo "$PIN_karpathy" ;;
    pstack) echo "$PIN_pstack" ;;
    speckit) echo "$PIN_speckit" ;;
    *) die "unknown repo: $1" ;;
  esac
}
url_of() {
  case "$1" in
    mattpocock) echo "$URL_mattpocock" ;;
    anthropics) echo "$URL_anthropics" ;;
    emilkowalski) echo "$URL_emilkowalski" ;;
    karpathy) echo "$URL_karpathy" ;;
    pstack) echo "$URL_pstack" ;;
    speckit) echo "$URL_speckit" ;;
    *) die "unknown repo: $1" ;;
  esac
}
sparse_of() {
  case "$1" in
    mattpocock) echo "$SPARSE_mattpocock" ;;
    anthropics) echo "$SPARSE_anthropics" ;;
    emilkowalski) echo "$SPARSE_emilkowalski" ;;
    karpathy) echo "$SPARSE_karpathy" ;;
    pstack) echo "$SPARSE_pstack" ;;
    *) die "unknown repo: $1" ;;
  esac
}

# Global skills dirs, one per detected harness home. Order is preference order.
detect_skills_targets() {
  [ -d "$HOME/.agents" ] && echo "$HOME/.agents/skills"
  [ -d "$HOME/.claude" ] && echo "$HOME/.claude/skills"
  [ -d "$HOME/.codex" ] && echo "$HOME/.codex/skills"
}

ensure_repo() { # $1=key $2=cache
  need_git
  local key="$1" cache="$2" dir rev url sparse
  dir="$cache/$key"; rev="$(pin_of "$key")"; url="$(url_of "$key")"; sparse="$(sparse_of "$key")"
  if [ -d "$dir/.git" ]; then
    if [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null)" = "$rev" ]; then return 0; fi
    if ! git -C "$dir" cat-file -e "$rev" 2>/dev/null; then
      git -C "$dir" fetch --depth 1 origin "$rev" || die "cannot fetch $rev for $key (offline?)"
    fi
  else
    mkdir -p "$cache"
    git clone --depth 1 --filter=blob:none --sparse "$url" "$dir" || die "cannot clone $url"
    git -C "$dir" sparse-checkout set "$sparse"
    if ! git -C "$dir" cat-file -e "$rev" 2>/dev/null; then
      git -C "$dir" fetch --depth 1 origin "$rev" || die "cannot fetch $rev for $key"
    fi
  fi
  git -C "$dir" checkout --detach -q "$rev"
}

skills_install() { # --target --cache --dry-run
  local target="auto" cache="$CACHE_DEFAULT" dry_run=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --target) target="$2"; shift 2 ;;
      --cache) cache="$2"; shift 2 ;;
      --dry-run) dry_run=1; shift ;;
      *) die "unknown flag: $1" ;;
    esac
  done
  local targets=()
  if [ "$target" = "auto" ]; then
    while IFS= read -r t; do [ -n "$t" ] && targets+=("$t"); done < <(detect_skills_targets)
    [ "${#targets[@]}" -gt 0 ] || die "no harness home detected (~/.agents ~/.claude ~/.codex); pass --target DIR"
  else
    targets=("$target")
  fi
  for key in mattpocock anthropics emilkowalski karpathy pstack; do
    [ "$dry_run" -eq 1 ] || ensure_repo "$key" "$cache"
  done
  local new=0 updated=0 same=0
  while IFS='|' read -r repo src dest _desc; do
    [ -n "$repo" ] || continue
    local from="$cache/$repo/$src"
    for t in "${targets[@]}"; do
      local to="$t/$dest"
      if [ "$dry_run" -eq 1 ]; then
        echo "would install $dest -> $to"
        continue
      fi
      [ -f "$from/SKILL.md" ] || die "missing $from/SKILL.md (cache: $cache)"
      if [ -d "$to" ] && diff -r -q "$from" "$to" >/dev/null 2>&1; then
        same=$((same + 1))
      else
        if [ -d "$to" ]; then updated=$((updated + 1)); else new=$((new + 1)); fi
        rm -rf "$to"
        mkdir -p "$t"
        cp -r "$from" "$to"
        chmod -R u+w "$to"
      fi
    done
  done <<<"$SKILLS"
  echo "skills: $new new, $updated updated, $same unchanged (${#targets[@]} target(s))"
}

skills_check() { # --target
  local target="auto"
  while [ $# -gt 0 ]; do
    case "$1" in
      --target) target="$2"; shift 2 ;;
      *) die "unknown flag: $1" ;;
    esac
  done
  local targets=()
  if [ "$target" = "auto" ]; then
    while IFS= read -r t; do [ -n "$t" ] && targets+=("$t"); done < <(detect_skills_targets)
    [ "${#targets[@]}" -gt 0 ] || die "no harness home detected; pass --target DIR"
  else
    targets=("$target")
  fi
  local missing=0 total=0
  for t in "${targets[@]}"; do
    while IFS='|' read -r _repo _src dest _desc; do
      [ -n "$_repo" ] || continue
      total=$((total + 1))
      if [ ! -f "$t/$dest/SKILL.md" ]; then
        echo "missing: $t/$dest"
        missing=$((missing + 1))
      fi
    done <<<"$SKILLS"
  done
  if [ "$missing" -gt 0 ]; then
    echo "skills check: $missing/$total missing" >&2
    return 1
  fi
  echo "skills check: $total present in ${#targets[@]} target(s)"
}

skills_list() {
  while IFS='|' read -r repo src dest desc; do
    [ -n "$repo" ] || continue
    printf '%-52s %-12s %s\n' "$dest" "$repo" "$desc"
  done <<<"$SKILLS"
  echo "---"
  echo "renamed on collision: emilkowalski-prototype, pstack-tdd, pstack-teach"
}

# spec-kit slash commands: project-local layout per harness (matches `specify init`).
agent_dest() { # $1=agent -> "layout|reldir"
  case "$1" in
    claude) echo "skills|.claude/skills" ;;
    codex) echo "skills|.agents/skills" ;;
    muse) echo "skills|.agents/skills" ;;
    cursor) echo "skills|.cursor/skills" ;;
    copilot) echo "skills|.github/skills" ;;
    opencode) echo "commands|.opencode/commands" ;;
    gemini) echo "commands|.gemini/commands" ;;
    *) die "unknown agent: $1" ;;
  esac
}
detect_agents() {
  [ -d "$HOME/.claude" ] && echo "claude"
  [ -d "$HOME/.codex" ] && echo "codex"
  [ -d "$HOME/.agents" ] && echo "muse"
  [ -d "$HOME/.cursor" ] && echo "cursor"
}
resolve_agents() { # $1=auto|all|name -> names, deduped by dest dir
  local sel="$1" names=()
  if [ "$sel" = "auto" ]; then
    while IFS= read -r a; do [ -n "$a" ] && names+=("$a"); done < <(detect_agents)
    [ "${#names[@]}" -gt 0 ] || die "no harness detected; pass --agent explicitly"
  elif [ "$sel" = "all" ]; then
    names=(claude codex cursor copilot opencode gemini muse)
  else
    agent_dest "$sel" >/dev/null
    names=("$sel")
  fi
  local seen="" a key
  for a in "${names[@]}"; do
    key="$(agent_dest "$a")"
    case " $seen " in
      *" $key "*) ;;
      *) seen="$seen $key"; echo "$a" ;;
    esac
  done
}

commands_install() { # --agent --dry-run
  local agent="auto" dry_run=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --agent) agent="$2"; shift 2 ;;
      --dry-run) dry_run=1; shift ;;
      *) die "unknown flag: $1" ;;
    esac
  done
  local src_dir="$ROOT/.specify/commands"
  [ -d "$src_dir" ] || die "$src_dir missing (spec-kit not vendored?)"
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    local spec dest_dir layout reldir
    spec="$(agent_dest "$a")"; layout="${spec%%|*}"; reldir="${spec##*|}"
    dest_dir="$ROOT/$reldir"
    for src in "$src_dir"/*.md; do
      local base name
      base="$(basename "$src" .md)"; name="speckit-$base"
      if [ "$layout" = "skills" ]; then
        local to="$dest_dir/$name/SKILL.md"
        if [ "$dry_run" -eq 1 ]; then echo "would install $a: $to"; continue; fi
        mkdir -p "$dest_dir/$name"
        # Ensure `name:` frontmatter for the skills layout; keep body verbatim.
        awk -v n="$name" 'NR==1 && $0=="---" {print; print "name: " n; next} {print}' "$src" >"$to"
      else
        local to="$dest_dir/$base.md"
        if [ "$dry_run" -eq 1 ]; then echo "would install $a: $to"; continue; fi
        mkdir -p "$dest_dir"
        cp -p "$src" "$to"
      fi
    done
    echo "commands: installed for $a ($reldir)"
  done < <(resolve_agents "$agent")
}

commands_check() { # --agent
  local agent="auto"
  while [ $# -gt 0 ]; do
    case "$1" in
      --agent) agent="$2"; shift 2 ;;
      *) die "unknown flag: $1" ;;
    esac
  done
  local missing=0 total=0
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    local spec layout reldir
    spec="$(agent_dest "$a")"; layout="${spec%%|*}"; reldir="${spec##*|}"
    for src in "$ROOT/.specify/commands"/*.md; do
      local base name to
      base="$(basename "$src" .md)"; name="speckit-$base"
      if [ "$layout" = "skills" ]; then to="$ROOT/$reldir/$name/SKILL.md"; else to="$ROOT/$reldir/$base.md"; fi
      total=$((total + 1))
      if [ ! -f "$to" ]; then echo "missing: $to"; missing=$((missing + 1)); fi
    done
  done < <(resolve_agents "$agent")
  if [ "$missing" -gt 0 ]; then
    echo "commands check: $missing/$total missing" >&2
    return 1
  fi
  echo "commands check: $total present"
}

hooks_install() {
  git -C "$ROOT" config core.hooksPath .githooks
  echo "hooks: core.hooksPath=.githooks"
}

pins() {
  need_git
  for key in mattpocock anthropics emilkowalski karpathy pstack speckit; do
    local pinned latest
    pinned="$(pin_of "$key")"
    latest="$(git ls-remote "$(url_of "$key")" HEAD 2>/dev/null | cut -f1)"
    [ -z "$latest" ] && latest="(unreachable)"
    printf '%-14s pinned %s\n%-14s latest %s\n' "$key" "$pinned" "" "$latest"
  done
}

cmd_verify() {
  local fail=0
  skills_check --target auto || fail=1
  commands_check --agent auto || fail=1
  if [ "$(git -C "$ROOT" config core.hooksPath 2>/dev/null)" = ".githooks" ]; then
    echo "hooks: configured"
  else
    echo "hooks: NOT configured (run: hooks install)"
    fail=1
  fi
  [ -d "$ROOT/.specify" ] && echo "spec-kit: vendored" || { echo "spec-kit: MISSING .specify/"; fail=1; }
  return $fail
}

cmd_all() { # --target --agent --cache
  local target="auto" agent="auto" cache="$CACHE_DEFAULT"
  while [ $# -gt 0 ]; do
    case "$1" in
      --target) target="$2"; shift 2 ;;
      --agent) agent="$2"; shift 2 ;;
      --cache) cache="$2"; shift 2 ;;
      *) die "unknown flag: $1" ;;
    esac
  done
  skills_install --target "$target" --cache "$cache"
  commands_install --agent "$agent"
  hooks_install
}

main() {
  [ $# -ge 1 ] || { usage; exit 1; }
  case "$1" in
    skills)
      [ $# -ge 2 ] || { usage; exit 1; }
      case "$2" in
        install) shift 2; skills_install "$@" ;;
        check) shift 2; skills_check "$@" ;;
        list) skills_list ;;
        *) usage; exit 1 ;;
      esac
      ;;
    commands)
      [ $# -ge 2 ] || { usage; exit 1; }
      case "$2" in
        install) shift 2; commands_install "$@" ;;
        check) shift 2; commands_check "$@" ;;
        *) usage; exit 1 ;;
      esac
      ;;
    hooks)
      [ "$2" = "install" ] || { usage; exit 1; }
      hooks_install
      ;;
    pins) pins ;;
    verify) cmd_verify ;;
    all) shift; cmd_all "$@" ;;
    -h | --help | help) usage ;;
    *) usage; exit 1 ;;
  esac
}

main "$@"

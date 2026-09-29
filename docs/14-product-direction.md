# 14 — Product Direction: cmux + herdr, t3code visual bar

signaltty is a native Linux workspace for parallel AI coding agents. Three
products set the bar; every feature proposal is judged against them.

- **cmux** ([manaflow-ai/cmux](https://github.com/manaflow-ai/cmux),
  [docs](https://cmux.com/docs)) — macOS-native AI-agent terminal multiplexer.
  Reference for the attention loop, inline approvals, agent hooks, and socket/CLI
  contract discipline.
- **herdr** ([herdrdev/herdr](https://github.com/herdrdev/herdr),
  [docs](https://herdr.dev/docs)) — open-source Rust agent multiplexer, tmux-shaped
  TUI. Reference for the agent-state sidebar, detection-as-data manifests, stable
  handles, and agents-as-API-clients (skill → CLI → raw socket).
- **t3code** ([pingdotgg/t3code](https://github.com/pingdotgg/t3code), MIT) — agent
  harness control surface (web/Electron/mobile). Reference for visual quality
  only: tokens, run rendering, diffs, micro-interactions.

## The seven directives

1. **Attention-first shell.** Ring/badge on panes, priority-sorted sidebar
   (blocked → done → working → idle), jump-to-latest-unread, OS notifications
   with inline actions. The product is the "which agent needs me" loop;
   everything else is table stakes (cmux notification rings + `⌘⇧U`, herdr
   sidebar with per-client seen/unseen done-states).
2. **Inline approvals, no context-switch.** Render the decision where the user
   already looks (cmux Feed: permission Once/Always/Deny, ExitPlanMode,
   AskUserQuestion). Never force the user into the agent's TUI to answer.
3. **Know thy agents two ways.** Hooks where CLIs offer them (cmux `hooks
   setup`: robust across CLI churn, buys session resume) + declarative
   per-agent screen-detection manifests where they don't (herdr
   `src/detect/manifests/*.toml`: data, not code, community-extensible).
   Never hardcode VT scraping in the GUI.
4. **Server owns sessions; everything is a client.** Named sessions,
   per-session sockets, detach/resume, layout + agent restore (herdr);
   versioned JSON socket API + CLI with stable handles, caller-context env,
   `--json` everywhere, a self-printing schema, cursor-based event replay +
   JSONL audit log (cmux `cli-contract.md` + `events.md`).
5. **Agents are API clients too.** Ship a gated skill file (`HERDR_ENV`-style:
   agents act only from inside a managed pane), agent→agent prompting, and a
   `wait`-for-blocked primitive. The multiplexer becomes an orchestration
   substrate: a primary agent fans out, waits, collects.
6. **Render work like t3code, not like `tail -f`.** Collapse-and-count streams,
   narrative prose headers alternating with typed one-line tool rows,
   per-turn-scoped diffs (`Latest turn ⌄`, `+N −N`, per-dir groups),
   same-component verb-tense run states (`Working for 2m…` → `Worked for 2m…`,
   no mode switch), summaries over token spam (streaming off by default).
   Even around terminal panes, sidebar/timeline/diff/approvals follow this
   language.
7. **Visual bar: Linear-grade tokens, native restraint.** Semantic color roles
   (canvas/surface/border/text/accent), elevation via lightness + 1px hairlines,
   system font stacks, low density with collapse escape hatches, thin
   monochrome icons with color reserved for status/brand, one hand-tuned drawer
   curve — expressed through libadwaita patterns (Adwaita light/dark, accent,
   toasts, sidebar) so the app still feels like a GNOME citizen.

## Deliberately not building

- cmux's Zen applies: ship a composable primitive (socket/CLI/events), let users
  find the workflows. No provider-matrix sprawl, no per-compositor special cases,
  until demand is proven.
- No Electron/web client: the GUI stays GTK4/libadwaita (see `06`); t3code is a
  visual reference, not an architecture reference.

## Sources

- cmux: `skills/cmux/SKILL.md`,
  `docs/{cli-contract,events,feed,notifications,agent-hooks,customizing-appearance}.md`,
  [Zen of cmux](https://cmux.com/blog/zen-of-cmux)
- herdr: `AGENTS.md`, `skills/herdr/SKILL.md`, `src/detect/manifests/`,
  `src/ui/sidebar.rs`, [socket API docs](https://herdr.dev/docs/socket-api/)
- t3code: `packages/shared/src/themePalettes.ts`,
  `apps/web/src/{index.css,appearanceFonts.ts}`,
  [t3code inventory](https://raw.githubusercontent.com/darkroomengineering/programa/HEAD/docs/plans/t3code-inventory.md)
  (darkroomengineering/programa)

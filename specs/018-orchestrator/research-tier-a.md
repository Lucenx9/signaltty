# Tier-A AI Orchestrators Research Report (October 2026)

## Executive Summary & Target Matrix

This report evaluates the top tier ("Tier-A") of AI coding orchestrators from an October 2026 practitioner benchmark: **Omnigent**, **Paseo**, **OnOrca (Orca)**, and **CodexHost**. It investigates how production systems coordinate multiple AI coding agents, manage repository isolation, detect turn boundaries without fragile terminal scraping, enforce safety and budget policies, and present a coherent developer UX.

All findings are grounded directly in cloned source repositories and official documentation under the strict verification standard:
- `VERIFIED-IN-SOURCE (<filepath>)`
- `FROM-DOCS (<url-or-docpath>)`
- `INFERRED`

### Comparative Architecture Matrix

| Dimension | Omnigent | Paseo | OnOrca (Orca) | CodexHost |
| :--- | :--- | :--- | :--- | :--- |
| **Repository & Commit** | `omnigent-ai/omnigent` (`5fae337`) | `getpaseo/paseo` (`8216e86`) | `stablyai/orca` (`d5bb69d3`) | `BytePioneer-AI/codex-host` (`49d8b1c`) |
| **Primary Stack** | Python 3.12+, FastAPI, tmux, Next.js | TypeScript / Node.js, node-pty, Expo / Electron | TypeScript, Electron, C++ native PTY, Monaco | TypeScript + Rust, loopback WebSocket / Unix socket |
| **Task / Unit Model** | Session hierarchy (`parent_session_id`), `sys_session_send`, Inbox queue | Markdown file store (`<id>.md`), DAG deps, acceptance criteria checklist | Structured sessions (`sessionId`, `paneKey`), worktree cards | Delegation entity (`delegationId`, parent/child `threadId`), Thread state |
| **Agent Isolation** | Sibling Git worktrees (`<repo>-worktrees/<branch>`), bwrap / seatbelt sandbox | Project-hashed Git worktrees (`~/.paseo/worktrees/<hash>/<slug>`), port allocator | Pre-warmed Git worktree pool (cap 3, 5m TTL), background trash removal | Cwd-targeted Thread Fork routing, delegated unattended execution presets |
| **Readiness & Turn Detection** | Preflight roster probe (`configured_harnesses`), subagent block notifier (120s grace) | Config hooks (`.claude/settings.json`, `.codex/hooks.json`), VT state machine | Unified host Agent Status Store, hook HTTP POSTs + PTY OSC stream parsing | App-server JSON-RPC client, `turn/completed` + visible final answer projection |
| **Prompt Delivery** | Async session stream, autonomous `/goal` condition | PTY stdin write, `send_agent_prompt` MCP tool, heartbeats / schedules | Native PTY write, headless pairing, remote SSH relay | `turn/steer` cancellation + restart folding, server-queue bypass |
| **Output Reading** | `sys_read_inbox` drain, tmux alt-screen vs scrollback capture | In-memory VT grid slice (`captureTerminalLines`), ANSI stripping | PTY transcript store, scrollback snapshot migrations, Monaco diff | `thread read --view result` (clean final message, tool/trace noise suppressed) |
| **Review & Merge Flow** | Polly lead agent never edits code; independent cross-model reviewer; worker PRs | Implement-then-review workflow, PR tracking, `auto-archive-on-merge` | Monaco side-by-side diffs, first-work branch rename, CI efficiency tracker | Multi-harness reviewer delegation (`@claude-code`, `@pi`, `@codex`), PR linking |
| **Cost & Concurrency** | Declarative policies (ALLOW/DENY/ASK), `cost_budget`, `sys_advise_models` | Agent profiles, worktree port isolation, workspace automation gates | Worktree preparation limits (3), SSH PTY leases, awake status leases | Memory-bounded JSONL chunking, token & cost telemetry surfaces |

---

## 1. Omnigent (`omnigent.ai`)

### Architecture & Stack
- **Core Server & Runner**: Implemented in Python 3.12+ using FastAPI, `asyncio`, and Uvicorn `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/server/app.py#L480-L550)`.
- **Terminal Control Bridge**: Interacts directly with `tmux` over dedicated Unix sockets. Terminal capture distinguishes between the primary screen buffer and alternate screen buffer to prevent leaking stale scrollback into TUI clients `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/terminals/control_bridge.py#L270-L330)`.
- **Frontend / Client**: Next.js / React web interface with WebSockets for real-time terminal streaming, session tree navigation, and interactive approval cards `VERIFIED-IN-SOURCE (/tmp/omnigent/web/package.json)`.
- **Meta-Harness Framework**: Ships with built-in agent personalities like `polly` (a hierarchical tech-lead orchestrator) and `debby` (cross-model debate) `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L1-L50)`.

### Task / Work-Unit Model & States
- **Hierarchical Sessions**: Work is structured as parent-child sessions linked by `parent_session_id` `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/entities/conversation.py)`.
- **Dispatches by Purpose**: Dispatches through `sys_session_send` require explicit operational categories:
  - `implement`: scoped repository changes that drive tests to green and publish a PR `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L95-L105)`.
  - `review`: judges an implementer's diff against an acceptance contract, taking file diffs and commit SHAs `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L106-L110)`.
  - `explore` / `search`: read-only investigations returning structured findings reports `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L111-L115)`.
- **Autonomous Goal Mode**: Long-running workers support `/goal <condition>` prefixing, executing continuously until the condition is satisfied `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L116-L125)`.

### Agent Spawn, Isolation & Worktree Lifecycle
- **Sibling Worktree Layout**: Worktrees are created strictly as siblings to the main repository root under `<parent>/<repo-name>-worktrees/<sanitized-branch>[-<suffix>]` `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L149-L165, L340-L365)`. This layout prevents nested worktree pollution when branching off linked worktrees.
- **Ref-Format Validation & Fetch Guarantee**: Branch names are strictly validated against `git check-ref-format` rules (no control characters, trailing periods, `.lock`, `@`, `..`, or leading slashes) `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L40-L75)`. If the base branch cannot be verified locally, it attempts a single `git fetch` before failing `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L370-L400)`.
- **Injection Defense**: All git commands use `--end-of-options` to prevent user-supplied branch names from injecting git flags `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L380-L390, L510-L520)`.
- **Cleanup & Pruning**: Worktree removal uses `git worktree remove --force <path>` and optionally deletes the branch with `git branch -D <branch>` `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L585-L630)`. Stale registrations are pruned automatically via `git worktree prune` before creating or re-attaching worktrees `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/host/git_worktree.py#L455-L465)`.
- **OS-Level Sandboxing**: Subprocess execution is wrapped in platform sandboxes: `bwrap` (bubblewrap) on Linux, `sandbox-exec` (Seatbelt) on macOS, and JobObjects on Windows `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/sandbox/bwrap.py, /tmp/omnigent/omnigent/sandbox/seatbelt.py)`.

### Readiness, Turn-Completion & Blocked Detection
- **Preflight Host Roster Check**: Before dispatching tasks, an orchestrator queries `sys_session_get_info({})` to read `configured_harnesses`. A harness is only invoked if readiness is strictly `true` (rejecting `"binary-missing"`, `"needs-auth"`, `"version-too-low"`) `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L65-L88)`.
- **Subagent Block Notifier with Escalation Grace**: When a subagent blocks on an approval (e.g. `response.elicitation_request`), Omnigent does not immediately wake the parent agent. Instead, it mirrors the approval card to the human UI and waits an escalation grace period (`_BLOCK_WAKE_ESCALATION_DELAY_S = 120.0s`). Only if the human fails to answer does it wake the parent agent to intervene `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/runtime/subagent_block_notifier.py#L1-L60)`.
- **Codex Native App Server RPC**: Spawns and manages Codex native app-server JSON-RPC over local WebSockets (`ws://localhost/rpc`). It injects a managed policy hook into `.codex/hooks.json` and completes hook trust verification (`hooks/trust` or `--dangerously-bypass-hook-trust` for Codex >= 0.131.0) `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/harnesses/codex_native/app_server.py#L100-L160)`.

### Prompt Delivery & Follow-Ups
- **Inbox-Driven Async Dispatch**: The orchestrator triggers worker tasks via `sys_session_send`. It does not block or poll; instead, completion notices land in the orchestrator's inbox `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L90-L95)`.
- **Follow-Up Routing**: Follow-ups reuse the existing subagent conversation by passing the active `session_id` back to `sys_session_send`.

### Output Reading
- **Non-Polling Inbox Queue**: The parent drains finished worker outcomes via `sys_read_inbox` `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/tools/builtins/async_inbox.py#L270-L290)`.
- **Terminal Control Seed**: For human terminal viewing, `_capture_pane_seed` runs `tmux capture-pane -e -p -J -t <target>`. On primary screens it captures full history (`-S -`), whereas on alternate screens (vim, claude, codex) it captures only visible screen cells to prevent corrupting terminal history `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/terminals/control_bridge.py#L270-L320)`.

### Structured Results & Parent/Child Hierarchy
- **Result Packaging**: Completed subagent runs yield structured verdicts, generated diff files, base/head commit SHAs, and pull request links `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L95-L115)`.
- **Persistence**: Parent/child relationships and session metadata are persisted in the SQL store (`conversation_store`) `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/stores/conversation_store/__init__.py)`.

### Review / Merge / PR / CI Flow
- **The "Lead Never Writes Code" Rule**: Polly strictly enforces that the tech lead orchestrator writes zero code and never directly merges code. All changes are authored by worker agents on isolated branches `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L30-L50)`.
- **Mandatory Independent Review**: Polly dispatches a separate, independent reviewer agent—explicitly running on a *different* model family (e.g. Claude Code checked by Codex or Pi)—to verify the diff against acceptance criteria before the PR is handed to the human for merging `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L12-L25)`.

### Concurrency & Cost Limits
- **Declarative Policy Engine**: Evaluates ALLOW, DENY, or ASK verdicts at three hierarchical levels: Session -> Agent Spec -> Server-wide `VERIFIED-IN-SOURCE (/tmp/omnigent/docs/POLICIES.md#L1-L35)`.
- **Cost Guardrails**: Built-in `cost_budget` policy halts or prompts for approval when USD spend reaches configured thresholds (`max_cost_usd`, `ask_thresholds_usd`) `VERIFIED-IN-SOURCE (/tmp/omnigent/docs/POLICIES.md#L45-L60)`.
- **Intelligent Routing Advice**: The `sys_advise_models` tool selects cost-effective models across tasks in a fan-out plan before dispatches are launched `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L85-L95)`.

### UX Highlights: What the User Sees & Does
- **Interactive Multi-Agent UI**: Users see an expandable Subagents tree in the sidebar displaying child agent status, active model, and execution purpose `VERIFIED-IN-SOURCE (/tmp/omnigent/designs/SESSION_PROJECTS_SIDEBAR.md)`.
- **Seamless Terminal Takeover**: Clicking any subagent opens its live tmux-backed terminal stream; users can take over typing directly into Claude Code or Codex TUI and return control to the agent at any point `VERIFIED-IN-SOURCE (/tmp/omnigent/examples/polly/config.yaml#L50-L60)`.
- **Mirrored Escalation Cards**: Subagent approval requests appear inline in the parent chat window; if the user approves there, the subagent immediately resumes without the parent agent needing to act `VERIFIED-IN-SOURCE (/tmp/omnigent/omnigent/runtime/subagent_block_notifier.py#L10-L30)`.

---

## 2. Paseo (`paseo.sh`)

### Architecture & Stack
- **Monorepo Structure**: TypeScript monorepo consisting of `@getpaseo/server` (daemon), `@getpaseo/protocol` (Zod schemas and wire types), `@getpaseo/cli` (CLI tool), `@getpaseo/app` (React Native / Expo / Unistyles cross-platform frontend), and `@getpaseo/desktop` (Electron shell) `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/package.json, /tmp/paseo/packages/protocol/package.json)`.
- **Terminal Subsystem**: Built on `node-pty` for real POSIX/Windows pseudo-terminals, coupled with an in-memory virtual terminal state machine maintaining scrollback and grid cells `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/terminal/terminal.ts#L1-L20, L940-L960)`.
- **E2EE Relay**: Includes `@getpaseo/relay` using WebSocket transport and cryptographic pairing keys for remote machine access `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/relay-transport.ts)`.

### Task / Work-Unit Model & States
- **Markdown Document Tasks**: Tasks are persisted as discrete Markdown files with YAML frontmatter in `~/.paseo/tasks/<id>.md` `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/tasks/task-document.ts#L5-L40, /tmp/paseo/packages/server/src/tasks/task-store.ts#L15-L35)`.
- **Task States**: Explicit status transitions: `"draft" | "open" | "in_progress" | "done" | "failed"` `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/tasks/types.ts#L1-L25)`.
- **Dependency DAG**: Tasks declare `deps: string[]` and optional `parentId`. The `TaskStore` calculates topological execution orders, ready tasks (`getReady`), and blocked tasks (`getBlocked`) `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/tasks/task-graph.ts, /tmp/paseo/packages/server/src/tasks/types.ts#L40-L55)`.
- **Checklist Acceptance Criteria**: Supports immutable `acceptanceCriteria: string[]` rendered as markdown checkboxes (`- [ ] ...`) and timestamped progress notes `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/tasks/task-document.ts#L30-L50)`.

### Agent Spawn, Isolation & Worktree Lifecycle
- **Project-Hashed Worktree Roots**: Worktrees are located under `<paseoHome>/worktrees/<project-hash>/<slug>` `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/utils/worktree.ts#L860-L900)`.
- **Rich Worktree Sources**: The worktree manager supports 6 distinct source variants:
  - `branch-off`: create branch from base `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/utils/worktree.ts#L180-L195)`.
  - `checkout-branch`: checkout existing branch.
  - `restore` / `restore-from-base`: recover existing worktree paths.
  - `checkout-change-request` / `checkout-github-pr`: directly checkout forge pull requests `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/utils/worktree.ts#L196-L215)`.
- **Lifecycle Hook Automation**: Runs repository-defined setup and teardown commands (`worktree.setup`, `worktree.teardown`) specified in `paseo.json` `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/utils/worktree.ts#L288-L295, L650-L680)`.
- **Dynamic Port Allocation**: Automatically allocates and reserves free network ports per worktree to prevent collision between parallel backend services `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/workspace-service-port-allocator.ts)`.

### Readiness, Turn-Completion & Blocked Detection
- **Config-Based Agent Hooks**: Installs lightweight event hook configurations into agent homes:
  - Claude Code: writes `.claude/settings.json` watching `UserPromptSubmit` (`running`), `Stop` / `StopFailure` / `SessionEnd` (`idle`), and `Notification` (parsing `idle_prompt` as `needs-input`) `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/terminal/agent-hooks/claude/claude.ts#L5-L40)`.
  - Codex: writes `.codex/hooks.json` watching `UserPromptSubmit`, `PreToolUse`, `PostToolUse` (`running`), `PermissionRequest` (`needs-input`), and `Stop` (`idle`) `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/terminal/agent-hooks/codex/codex.ts#L5-L35)`.
  - OpenCode: installs an OpenCode plugin hook `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/terminal/agent-hooks/opencode/opencode.ts)`.
- **Normalized Activity State Machine**: Normalizes all terminal agents into three states: `idle`, `working`, and `attention` (with attention reasons: `finished` or `needs_input`) `VERIFIED-IN-SOURCE (/tmp/paseo/packages/protocol/src/terminal-activity.ts#L3-L25)`.

### Prompt Delivery & Follow-Ups
- **Direct PTY Injection & Tool Calls**: Agents send prompts to other agents via the `send_agent_prompt` MCP tool or CLI `paseo send <agentId>` `FROM-DOCS (/tmp/paseo/public-docs/orchestration.md)`.
- **Heartbeats & Schedules**: Supports periodic heartbeats prompting the same agent on a cadence (e.g. every 10 min) to continue long migrations without losing context `FROM-DOCS (/tmp/paseo/public-docs/orchestration-workflows.md#keep-an-agent-working-with-a-heartbeat)`.

### Output Reading
- **In-Memory VT Screen Capture**: Maintains a full grid and scrollback buffer of terminal cells. `captureTerminalLines` slices lines by `start` and `end` indices and strips ANSI escapes via `strip-ansi` `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/terminal/terminal-capture.ts#L10-L65)`.

### Structured Results & Parent/Child Hierarchy
- **Subagents Track**: Child agents appear in a dedicated Subagents track near the prompt composer `FROM-DOCS (/tmp/paseo/public-docs/orchestration.md#where-the-work-appears)`.
- **Full Bidirectional Sessions**: Unlike read-only subagent logs, every Paseo subagent is a full conversational session that can be addressed directly, paused, or detached into a standalone top-level agent via `paseo agent detach` `FROM-DOCS (/tmp/paseo/public-docs/orchestration.md)`.

### Review / Merge / PR / CI Flow
- **Checkout Diff Manager**: Tracks file diffs per worktree against the base branch `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/checkout-diff-manager.ts)`.
- **Automatic Archive on Merge**: Observes repository git references and automatically archives the agent session and prunes the worktree when its PR is merged `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/auto-archive-on-merge)`.

### Concurrency & Cost Limits
- **Workspace Automation Gates**: Enforces rate limits and process limits on automated subagent spawns `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/workspace-automation-gate.ts)`.
- **Port Allocation Fencing**: Bounds concurrent service execution through the port allocator pool `VERIFIED-IN-SOURCE (/tmp/paseo/packages/server/src/server/workspace-service-port-registry.ts)`.

### UX Highlights: What the User Sees & Does
- **Subagents Track Beside Composer**: Subagent cards are displayed directly above/beside the chat composer with live state pills (`running`, `needs_input`, `attention`) `FROM-DOCS (/tmp/paseo/public-docs/orchestration.md)`.
- **Universal Mobile & Desktop Parity**: Built with React Native / Expo and Unistyles, providing identical orchestration capabilities on mobile devices (iOS/Android) via encrypted relay `FROM-DOCS (/tmp/paseo/docs/unistyles.md, /tmp/paseo/docs/mobile-testing.md)`.
- **One-Click Agent Detach**: Any subagent running in a nested workspace can be detached into a first-class top-level tab at will `FROM-DOCS (/tmp/paseo/public-docs/orchestration.md)`.

---

## 3. OnOrca (`onorca.dev` / Orca)

### Architecture & Stack
- **Desktop Core**: High-performance Electron application with a React frontend and native C++ node addons for high-throughput PTY handling `VERIFIED-IN-SOURCE (/tmp/orca/src/main, /tmp/orca/src/renderer)`.
- **Headless Server (`orcad`)**: Standalone headless daemon for Linux and remote servers, providing full agent hosting without requiring an active graphical desktop `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/headless-linux-server.md, /tmp/orca/docs/reference/orcad-operations.md)`.
- **Monaco Code Editor Integration**: Embeds the full Monaco editor for in-app code editing, side-by-side git diff review, and inline comment authoring `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/monaco-language-associations.md)`.
- **Remote Bridge**: Multi-host orchestration bridging local processes, WSL distros, and remote SSH hosts through unified wire protocols `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/ssh-execution-boundary.md, /tmp/orca/docs/reference/wsl-hook-relay-manager.ts)`.

### Task / Work-Unit Model & States
- **Worktree-Centric Cards**: Work units are mapped directly to Git worktree cards, tracking branch status, commit progress, and associated pull requests `VERIFIED-IN-SOURCE (/tmp/orca/src/main/persistence-worktree-card-properties.test.ts)`.
- **Dual Session Model**: Supports both unstructured PTY terminal sessions (`paneKey`) and structured native-chat agent sessions (`sessionId`), reconciling them under a single authority `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/agent-status-store.md#L30-L50)`.

### Agent Spawn, Isolation & Worktree Lifecycle
- **Pre-Warmed Worktree Preparation Pool**: Eliminates git worktree creation latency by maintaining a background pool of pre-warmed worktrees (default cap: 3, TTL: 5 minutes) created off canonical base refs (`HEAD` / default branch) `VERIFIED-IN-SOURCE (/tmp/orca/src/main/worktree-create-preparation-pool.ts#L45-L65)`. When an agent spawns, it claims a pre-warmed worktree instantaneously instead of blocking on `git worktree add`.
- **Tip Refresh Queue**: If upstream commits arrive while a prepared worktree sits idle in the pool, the `worktree-preparation-refresh-queue` fetches and fast-forwards the pool worktrees in the background `VERIFIED-IN-SOURCE (/tmp/orca/src/main/worktree-preparation-refresh-queue.ts)`.
- **Asynchronous Background Removal & Trash**: To prevent UI freezes during large repository deletions, deleted worktrees are moved to a trash tombstone directory and pruned asynchronously `VERIFIED-IN-SOURCE (/tmp/orca/src/main/worktree-background-removal.ts, /tmp/orca/src/main/worktree-trash.ts)`.
- **PTY Descendant Process Termination**: When a terminal pane or worktree is retired, Orca walks the OS process tree to forcefully terminate all orphaned background child processes, preventing leaked node/python workers from holding file locks on worktree directories `VERIFIED-IN-SOURCE (/tmp/orca/src/main/pty-descendant-termination.ts#L1-L50)`.

### Readiness, Turn-Completion & Blocked Detection
- **Canonical Execution Host Agent Status Store**: Implements the architectural principle: *"The execution host owns agent status, in one store, and every reader subscribes to it"* `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/agent-status-store.md#L45-L65)`.
- **Multi-Source Event Ingestion**: Converges hook HTTP POSTs, WSL/SSH relays, and terminal stream OSC parse events (`applyNormalizedStatus`) stamped with host authority `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/agent-status-store.md#L65-L80)`.
- **Comprehensive Harness Adapters**: Main process includes dedicated status normalization and lifecycle monitors for Claude Code, Codex, OpenCode, Cursor, Gemini, Copilot, Grok, Pi, and Antigravity `VERIFIED-IN-SOURCE (/tmp/orca/src/main/agent-hooks/server/)`.

### Prompt Delivery & Follow-Ups
- **PTY Stream Multiplexing**: Writes input directly to the master PTY descriptor with configurable terminal flow controls and telemetry logging `VERIFIED-IN-SOURCE (/tmp/orca/src/main/agent-hooks/server/server-prompt-sent-telemetry.test.ts)`.
- **First-Work Branch Renaming**: On initial prompt submission, automatically generates a meaningful git branch name based on the task description and renames the underlying worktree branch `VERIFIED-IN-SOURCE (/tmp/orca/src/main/agent-hooks/first-work-branch-rename.ts#L1-L60)`.

### Output Reading
- **PTY Transcript Capture & Snapshotting**: Streams terminal bytes into disk-backed transcript stores with async scrollback snapshot migration `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/agent-pty-transcript-capture.md, /tmp/orca/src/main/terminal-scrollback-snapshots.ts)`.
- **High-Throughput xterm Patches**: Uses custom xterm.js optimizations to render massive output streams without dropping frames or freezing the renderer `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/xterm-patch-regeneration.md)`.

### Structured Results & Parent/Child Hierarchy
- **Structured Agent Session Status Feed**: Provides broadcast status feeds for structured subagent runs, reporting current task step, active tool, and turn completion `VERIFIED-IN-SOURCE (/tmp/orca/src/main/agent-hooks/server/server-structured-canonical-status.test.ts)`.
- **Hydration Confirmation**: Restored subagents from disk carry a `restoredUnconfirmed` flag until verified against live host processes, preventing ghost sessions after crashes `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/agent-status-store.md#L75-L85)`.

### Review / Merge / PR / CI Flow
- **Built-in Monaco Review Workspace**: Users review staged and unstaged worktree diffs side-by-side directly in the application before staging or committing `FROM-DOCS (/tmp/orca/docs/reference/monaco-language-associations.md)`.
- **CI Demand Rollout & Monitoring**: Tracks CI run durations and runner efficiency across worktree pull requests `VERIFIED-IN-SOURCE (/tmp/orca/docs/reference/ci-runner-efficiency.md)`.

### Concurrency & Cost Limits
- **Worktree Preparation Limits**: Pools are hard-capped at 3 idle checkouts `VERIFIED-IN-SOURCE (/tmp/orca/src/main/worktree-create-preparation-pool.ts#L46)`.
- **Remote PTY Leases**: Leases SSH and remote daemon PTYs with strict timeout policies to avoid exhausting remote server resources `VERIFIED-IN-SOURCE (/tmp/orca/src/main/persistence-ssh-remote-pty-leases.test.ts)`.

### UX Highlights: What the User Sees & Does
- **Native Multi-Pane Grid & Worktree Bar**: The top bar displays all active worktrees; clicking a card immediately switches the workspace context, terminal grid, and Monaco editor `INFERRED from UI components in /tmp/orca/src/renderer`.
- **Seamless Local/Remote Unification**: Local workspaces, WSL distros, and SSH servers appear identically in the sidebar with live agent activity spinners `FROM-DOCS (/tmp/orca/docs/reference/headless-linux-server.md)`.
- **Mobile QR Pairing**: Display a QR code in desktop settings to pair an iOS/Android device over encrypted relay in seconds `VERIFIED-IN-SOURCE (/tmp/orca/src/main/pairing-qr.ts)`.

---

## 4. CodexHost (`github.com/BytePioneer-AI/codex-host`)

### Architecture & Stack
- **Hybrid Rust + TypeScript Monorepo**: Rust handles low-level process launchers, platform shims, and memory-safe subprocess management (`crates/launcher`, `crates/platform`), while TypeScript coordinates the protocol runtime and desktop hooks (`packages/host-runtime`, `packages/harness-adapter`, `packages/protocol-core`) `VERIFIED-IN-SOURCE (/tmp/codex-host/package.json, /tmp/codex-host/Cargo.toml)`.
- **App-Server Protocol Interceptor**: Sits as a transparent proxy between the official Codex Desktop GUI and local/remote AI agent harnesses. Communicates via WebSocket, Unix domain socket, or Windows named pipes `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/app-server-transport.md)`.
- **Big Message JSONL Streaming**: Safely processes messages exceeding 128 MiB (such as extensive tool call outputs or image attachments) without buffering memory bloat or socket disconnects `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/app-server-transport.md)`.

### Task / Work-Unit Model & States
- **First-Class Delegation Entity**: Manages task delegation via explicit `Delegation` records linking `delegationId`, `parentThreadId`, `childThreadId`, target `harnessId`, and `status` (`pending`, `running`, `completed`, `failed`, `cancelled`) `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L110-L150)`.
- **Idempotency Window**: Calls to `delegate start` can supply an explicit `--request-id` or rely on a time-bounded hash of `(parentThreadId, taskText)` to prevent duplicate subagent runs on transient network retries `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L115-L135)`.
- **Interactive Top-Level Status**: Delegated child threads are created as standard, interactive Codex Desktop threads. Users can click them in the session list, converse with them, and inspect their execution `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L80-L105)`.

### Agent Spawn, Isolation & Worktree Lifecycle
- **External Thread Worktree Fork Routing**: Accepts a Desktop-provisioned target cwd for thread forks without making CodexHost itself manage low-level git commands. When an external Thread Fork carries an absolute cwd different from the source, Host creates the derived Thread and Native Session in that worktree directory `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/external-thread-worktree-fork-routing/spec.md#L10-L40)`.
- **Unattended Execution Presets**: Automatically configures subagent safety profiles for headless execution:
  - DeepSeek Harness: executes `/permission danger-full-access` before dispatch `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L65-L75)`.
  - Native Codex: starts thread with `approvalPolicy: "never"` and `sandbox: "danger-full-access"` `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L76-L85)`.
  - Cursor CLI: invokes `--force acp` in unattended mode; if an interactive prompt is still triggered, it fails loud rather than hanging indefinitely `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L86-L98)`.

### Readiness, Turn-Completion & Blocked Detection
- **Turn Activity Folding**: Analyzes agent execution items and automatically projects `HostAgentMessageItem.phase = "final_answer"` onto the terminal agent message. This enables Codex Desktop to fold all intermediate reasoning, tool calls, and shell executions behind a clean `"Worked for {time}"` collapsible UI row `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/turn-activity-folding.md#L1-L45)`.
- **Commentary Phase Projection**: If an agent outputs commentary or an error prior to completion, it is tagged as `phase: "commentary"` to prevent improper UI folding `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/turn-activity-folding.md#L50-L75)`.
- **Cross-Harness Adapter Discovery**: Dynamic adapter registry automatically detects locally installed agent CLIs (`claude`, `codex`, `opencode`, `cursor`, `hermes`, `pi`, `kiro`, `qoder`, `deepseek`) and establishes RPC bridges `VERIFIED-IN-SOURCE (/tmp/codex-host/packages/harness-discovery)`.

### Prompt Delivery & Follow-Ups
- **External Turn Steering (`turn/steer`)**: Seamlessly intercepts the user steering button in the UI:
  1. Issues cancellation to the active turn.
  2. Bounded wait (up to 20s) for turn cleanup and item closure.
  3. Automatically starts a new turn with the replacement prompt under a new Turn ID.
  4. Keeps UI history intact without creating corrupted partial transcripts `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/external-thread-steering.md#L1-L45)`.
- **Follow-Up Message Sending**: Provides `codexhost thread send <thread> --message <text>` to deliver subsequent instructions to any existing writable thread `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L280-L310)`.

### Output Reading
- **Filtered Visible Result Reading (`thread read`)**: `codexhost thread read <thread> [--view result|messages]` returns a concise, high-level summary:
  - `result.availability`: `"available" | "pending" | "unavailable"`
  - `result.text`: the final agent message of the turn `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L200-L245)`.
  - **Context-Window Protection**: By default, `thread read` **strictly omits** intermediate tool calls, tool parameters, terminal outputs, file diffs, and reasoning summaries, ensuring that parent orchestrator context is not contaminated with execution spam `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L200-L215)`.

### Structured Results & Parent/Child Hierarchy
- **Bounded Waiting (`thread wait`)**: `codexhost thread wait <thread> [--timeout-ms <n>]` allows parent agents to wait for child completion synchronously up to a deadline, returning `{ timedOut: boolean, result }` `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L250-L275)`.
- **One-Off Event Watches (`thread watch`)**: Parents can register a watch: `codexhost thread watch <childThread> --notify <parentThread>`. When the child finishes or errors, CodexHost injects an event turn into the parent thread without busy-polling `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L160-L195)`.

### Review / Merge / PR / CI Flow
- **Multi-Harness Delegation Pipeline**: Orchestrators delegate implementation to one harness (e.g. `@claude-code`) and independent code review to another (e.g. `@pi` or `@codex`) using agent skills `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/cross-harness-delegation/spec.md#L10-L40)`.
- **Desktop PR Integration**: Hooks into Codex Desktop's pull request panel to surface linked branches and PR status.

### Concurrency & Cost Limits
- **Telemetry Surfaces**: Exposes model token usage, cache hit rates, and USD session costs directly to Desktop UI surfaces (`renderer-thread-usage-surface`) `VERIFIED-IN-SOURCE (/tmp/codex-host/openspec/specs/renderer-thread-usage-surface)`.
- **Memory-Safe JSONL Framing**: Bounds buffer usage per connection to prevent OOMs when dozens of external threads stream simultaneously `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/app-server-transport.md)`.

### UX Highlights: What the User Sees & Does
- **Native Codex Desktop UI**: All external agents (Claude Code, Pi, OpenCode, Hermes) render identically to official OpenAI Codex threads inside the Codex Desktop application `VERIFIED-IN-SOURCE (/tmp/codex-host/packages/renderer-extension)`.
- **"Worked for {time}" Folding**: Thoughts, commands, and intermediate steps collapse cleanly, showing only the concise final answer until expanded `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/turn-activity-folding.md)`.
- **Live Steering & Queueing**: Users can hit "Steer", edit queued messages, or trigger manual compaction (`/compact`) on any third-party agent thread seamlessly `VERIFIED-IN-SOURCE (/tmp/codex-host/docs/architecture/external-thread-steering.md#L60-L90)`.

---

## 5. Why They Rank Above the Rest

Lower-tier orchestrators (such as raw terminal multiplexers or basic scripts like cmux, herdr, workmux, Conductor, and vibe-kanban) rely on brittle screen scraping, naive sleep loops, shared working trees, and monolithic single-agent loops. The Tier-A orchestrators rank at the top of the benchmark due to four fundamental architectural breakthroughs:

1. **Protocol-Level Hooks & App-Server Interception over Screen Scraping**:
   - Instead of parsing ANSI escape sequences or regex-matching shell prompts (`❯`, `$`, `?`), Tier-A systems hook directly into structured lifecycle interfaces:
     - Codex JSON-RPC app-server websockets (`codex-host`, `omnigent`).
     - Agent configuration event hooks (`.claude/settings.json`, `.codex/hooks.json` in `paseo` and `orca`).
     - Ingestion servers receiving authoritative JSON events (`UserPromptSubmit`, `Stop`, `PermissionRequest`, `idle_prompt`).
   - This guarantees 100% deterministic detection of whether an agent is actively working, awaiting input, or finished.

2. **Advanced Git Worktree Lifecycle Management**:
   - Rather than forcing agents to share a dirty repository or running naive `git worktree add`, Tier-A tools engineer high-throughput worktree infrastructure:
     - **Pre-warmed Preparation Pools** (OnOrca): pre-allocates up to 3 worktrees in the background, eliminating agent spawn latency.
     - **Sibling Worktree Layouts** (Omnigent): standardizes `<repo>-worktrees/<sanitized-branch>`, enforcing `--end-of-options` to block flag injection.
     - **Asynchronous Background Teardown & Trash** (OnOrca, Paseo): moves deleted worktrees to trash tombstones, deleting multi-gigabyte directories in the background without UI or PTY hiccups.
     - **Dynamic Port & Process Isolation** (Paseo, OnOrca): allocates dedicated service ports and crawls process trees to murder leaked background descendants.

3. **Context-Protecting Delegation Contracts & Non-Polling Wakeups**:
   - In naive multi-agent scripts, when a child agent completes, its entire terminal dump or raw log is dumped back into the parent prompt, quickly exhausting context windows and triggering hallucinations.
   - Tier-A systems strictly decouple execution traces from parent observation:
     - CodexHost's `thread read --view result` strips all intermediate tool calls and returns only `result.availability` and the synthesized final text.
     - Omnigent's `sys_read_inbox` and CodexHost's `thread watch` wake parent orchestrators reactively via event queues rather than busy spin-polling.
     - Omnigent's subagent block notifier incorporates an escalation grace period (120s), allowing humans to approve child tool calls silently in the UI before bothering the parent agent.

4. **Multi-Model Role Specialization & Cross-Vendor Verification**:
   - Tier-A orchestrators institutionalize the separation of tech lead from implementer:
     - In Omnigent Polly, the tech lead *never* edits code directly. It decomposes tasks, delegates to specialized CLIs, and requires an independent cross-model review (e.g. Claude Code checked by Codex or Pi) before opening a PR.
     - Paseo provides automated implement-then-review workflows, while CodexHost allows seamless `@harness` routing across 9+ agent engines within the same desktop interface.

---

## 6. Deltas for Feature 018 Orchestrator (Ranked)

These concrete recommendations are ranked by impact for Signaltty's Feature 018 (`orchestrator`):

### Rank 1: Add Filtered Clean Result View to `pane.read` / Task Completion
- **Observation**: CodexHost (`thread read --view result`) and Omnigent demonstrate that feeding intermediate tool calls, terminal progress bars, and ANSI scrollback back to an orchestrating agent causes severe LLM context pollution and hallucinations.
- **Delta for 018**:
  - In `contracts/ipc.md` under `pane.read`, add an optional field `view?: "raw" | "result"` (defaulting to `"raw"` for backwards compatibility).
  - When `view == "result"` is requested (or when querying `task.get` on a completed task), return only the agent's synthesized final response text and availability status (`available | pending | unavailable`), omitting intermediate ANSI redraws, shell prompts, and tool output noise.

### Rank 2: Add Attention Reason Taxonomy & Escalation Grace Period
- **Observation**: Omnigent (`subagent_block_notifier.py`) mirrors permission cards to the human UI and delays parent wakeups by 120s. Paseo (`terminal-activity.ts`) distinguishes `attention` reasons between `finished` and `needs_input`.
- **Delta for 018**:
  - Extend `attention.pending` in `contracts/ipc.md` and `data-model.md` to include `reason: "needs_input" | "turn_finished"`.
  - Add an orchestrator configuration setting `escalation_grace_ms: u64` (e.g., 60,000ms). When a worker enters `input_required`, notify the GTK UI immediately, but defer waking the parent orchestrator agent until the grace period elapses without human intervention.

### Rank 3: Enforce Sibling Worktree Path Structure & Git Ref Sanitization
- **Observation**: Omnigent (`git_worktree.py`) and Paseo (`worktree.ts`) show that creating worktrees inside repository trees causes recursive nesting bugs, while unescaped branch names permit git flag injection.
- **Delta for 018**:
  - In `task.start`, enforce that auto-provisioned worktree roots are created strictly as siblings to the main repository (e.g., `<parent>/<repo>-worktrees/<sanitized-task-slug>`).
  - Add strict branch name sanitization conforming to `git check-ref-format` and always pass `--end-of-options` to git subprocess invocations.

### Rank 4: Bounded Wait Primitives in IPC
- **Observation**: CodexHost provides `thread wait --timeout-ms <n>` returning `{ timedOut: bool, result }` to eliminate busy-polling loops in orchestrator scripts.
- **Delta for 018**:
  - Add an optional `timeout_ms?: u64` parameter to `task.start` or introduce a lightweight `task.wait(task_id, timeout_ms)` IPC method. If the task completes before the timeout, return immediately; otherwise return `{ state: "working", timed_out: true }`.

---

## 7. Ideas for Feature 019 Board / PR-CI Follow-up

1. **Pre-Warmed Worktree Preparation Pool** (from OnOrca):
   - Implement a background pool in Signaltty that keeps 1–2 clean worktrees pre-checked out on `HEAD` of the primary branch, cutting `task.start` latency down to ~0ms.
2. **Turn Activity Folding in Signaltty GTK Workspace** (from CodexHost):
   - In the terminal pane renderer, visually fold thought blocks, raw command executions, and repetitive tool calls behind an interactive `"Worked for Xs"` collapsible banner once the turn reaches `idle`.
3. **Asynchronous Worktree Teardown & Trash Tombstones** (from OnOrca):
   - Defer worktree deletion on `task.stop` / `task.discard` to a background thread pool that moves directories to `~/.cache/signaltty/trash` first, keeping GTK UI and PTY dispatchers completely jank-free.
4. **Automated Cross-Vendor Review Pipeline** (from Omnigent Polly & Paseo):
   - Add a workflow stage where, upon implementer task completion, an independent reviewer agent (configured with a different LLM family) is automatically spawned into the worktree to review the staged diff against the task acceptance criteria.
5. **Auto-Archive on Git Merge** (from Paseo):
   - Monitor git refs or GitHub PR webhooks to automatically close terminal panes and clean up worktrees when a task branch is merged upstream.

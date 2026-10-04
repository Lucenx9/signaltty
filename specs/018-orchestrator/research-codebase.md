# Codebase Audit: Orchestrator Prerequisites (Feature 018)

**Audit Date**: 2026-10-04  
**Target Branch**: `orch/tasks`  
**Repository**: `/home/simone/code/signaltty`  
**Auditor**: Research Sub-agent (Feature 018)

---

## 1. `signaltty pane read` in `tail` Mode on Full-Screen TUIs

### Root Cause Analysis
In `signaltty-server`, `h_pane_read` ([`crates/signaltty-server/src/router.rs:949-980`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L949-L980)) dispatches between two modes:
1. `ReadMode::Tail`: Calls `terms.tail(&id, n, strip)` and joins lines with `\n`.
2. `ReadMode::Screen`: Calls `terms.snapshot(&id)` and optionally strips ANSI.

In `signaltty-term` ([`crates/signaltty-term/src/headless.rs:24-80, 110-125`](file:///home/simone/code/signaltty/crates/signaltty-term/src/headless.rs#L24-L80)), the headless terminal engine processes incoming bytes in `Surface::feed`:
```rust
// crates/signaltty-term/src/headless.rs:60-78
for &b in data {
    if b == b'\n' {
        let line = std::mem::take(&mut self.fragment);
        self.push_line(line);
    } else {
        self.fragment.push(b as char);
    }
}
self.parser.process(data);
```
- **Byte Stream Splitting vs. Virtual Terminal**: `Surface::feed` independently splits incoming raw bytes on `\n` into `self.lines: VecDeque<String>` while feeding `data` to `vt100::Parser`.
- **TUI Cursor Semantics Ignored**: Interactive full-screen TUIs (Claude Code, Codex, Ink/Pastel, Bubbletea) repaint and move the cursor using ANSI escape sequences (`\x1b[H`, `\x1b[row;colH`), overwrite lines using Carriage Return (`\r`), clear lines (`\x1b[K`), or switch to the alternate screen buffer (`\x1b[?1049h`). They rarely emit `\n` during inline updates or repaints.
- **Escape Stripping Concatenation**: When `strip = true`, `signaltty_term::sanitize::strip_ansi` ([`crates/signaltty-term/src/sanitize.rs:17-64`](file:///home/simone/code/signaltty/crates/signaltty-term/src/sanitize.rs#L17-L64)) elides CSI/OSC escape codes via regex/state scanning. Because escape codes specifying cursor positions (such as column offsets or jumps back to row 0) are stripped without moving a cursor, characters drawn at different positions or repainted headers collapse onto the same line (e.g. `Header 1Header 2` or `Status: StartingStatus: Working Status: Ready`). Furthermore, every screen repaint appends to the un-cleared `self.lines` queue, repeating banners and UI chrome across the output.

### Reproduction Evidence
Feeding a typical TUI banner and status update:
`\x1b[2J\x1b[H=== Agent CLI ===\r\nStatus: Starting\rStatus: Working \rStatus: Ready   \r\n\x1b[H=== Agent CLI [Active] ===`
- **Tail Mode Output**:
  ```text
  === Agent CLI ===
  Status: StartingStatus: Working Status: Ready   
  === Agent CLI [Active] ===
  ```
  `\r` is preserved or joined within the line; overwrites concatenate instead of replacing; repositioning duplicates text.
- **Screen Mode Output**:
  `s.parser.screen().contents()` correctly maintains the 2D grid: row 0 shows `=== Agent CLI [Active] ===` and row 1 shows `Status: Ready   `.

### Screen Mode Limitations & Better Design
- **Zero Scrollback in Headless Engine**: In [`crates/signaltty-term/src/headless.rs:32`](file:///home/simone/code/signaltty/crates/signaltty-term/src/headless.rs#L32), `Surface::new` initializes the parser with `vt100::Parser::new(rows, cols, 0)`. The `vt100` parser retains **0** scrollback rows. Lines that scroll off the top of the visible screen are discarded immediately.
- **Alternate Screen Isolation**: When an agent runs in the alternate screen buffer (`\x1b[?1049h`), `vt100` maintains an isolated `alternate_grid` with no scrollback.
- **Architectural Solution for Orchestrator (018)**:
  1. Increase `vt100::Parser` scrollback capacity (e.g., 5,000 lines) in `Surface::new`.
  2. Implement structured grid extraction: extract rendered lines from the virtual terminal scrollback + visible screen (e.g., `screen.rows()` or `screen.scrollback()`), rather than naively chunking raw uninterpreted byte streams on `\n`.
  3. Support incremental extraction: track cursor position or compare rendered snapshots (`screen().contents_diff()`) between execution steps.

---

## 2. `pane.input`, Carriage Returns, Bracketed Paste, and Agent Readiness

### Input Byte Handling
- **CLI Serialization**: [`crates/signaltty-cli/src/main.rs:998-1014`](file:///home/simone/code/signaltty/crates/signaltty-cli/src/main.rs#L998-L1014) (`PaneOp::Input`) reads `--data` or `--stdin` into `Vec<u8>` and encodes it as base64 into `data_b64`.
- **Server Dispatch**: [`crates/signaltty-server/src/router.rs:871-903`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L871-L903) (`h_pane_input`) validates that the pane is in `LiveState::Live`, decodes base64, and calls `ctx.ptys.input(&id, &data)`.
- **PTY Write**: [`crates/signaltty-server/src/pty.rs:342-351`](file:///home/simone/code/signaltty/crates/signaltty-server/src/pty.rs#L342-L351) (`Ptys::input`) writes the raw bytes directly to the PTY master descriptor:
  ```rust
  h.writer.write_all(data).map_err(|e| e.to_string())?;
  h.writer.flush().map_err(|e| e.to_string())?;
  ```

### Newline (`\n`) vs Carriage Return (`\r`) & Submission
- When a user or script runs `signaltty pane input <id> --data "hi\n"`, the CLI writes bytes `[b'h', b'i', b'\n']`.
- In raw/cbreak terminal modes (used by interactive shells like bash/zsh with readline, as well as Claude Code and Codex TUIs), `ICRNL` translation is disabled.
- **Linefeed (`\n`, 0x0A)** is treated as a newline insertion or down-cursor movement, **not a submission**.
- **Carriage Return (`\r`, 0x0D)** is the character that submits prompts and sends the command. Writing only `\n` fails to submit prompts in interactive TUIs.

### Bracketed Paste Mode
- **No Bracketed Paste in Server Today**: `pane.input` writes raw bytes directly with zero wrapping or awareness of terminal modes.
- **TUI Behavior (Claude Code / Codex)**:
  - Both Claude Code and Codex enable bracketed paste (`\x1b[?2004h`) in their terminal UI loops.
  - When text is wrapped inside bracketed paste markers (`\x1b[200~` ... `\x1b[201~`), any `\r` or `\n` inside the brackets is interpreted as an inline multiline newline within the text editor, **never as a submission**.
  - To paste multiline prompts without premature execution, text must be enclosed in bracketed paste markers, followed by an explicit `\r` outside and after the closing marker:
    `\x1b[200~<prompt_text>\x1b[201~\r`
  - In practice, many TUIs require a small delay (or separate event dispatch) between closing bracketed paste and sending `\r` to allow the UI event loop to commit the pasted buffer before handling Enter.

### Agent "Ready for Input" Concept
- **Current Model**: Defined in [`crates/signaltty-core/src/state.rs:10-23`](file:///home/simone/code/signaltty/crates/signaltty-core/src/state.rs#L10-L23):
  `Lifecycle`: `Unknown`, `Working`, `Blocked`, `Done`, `Idle`, `Failed`, `Exited`.
  `Attention`: `None`, `Unread`, `InputRequired`, `PermissionRequired`, `Warning`, `Error`.
- **Adapter Event Mappings**:
  - Claude ([`crates/signaltty-agent/src/adapters/claude.rs:25-85`](file:///home/simone/code/signaltty/crates/signaltty-agent/src/adapters/claude.rs#L25-L85)):
    - `SessionStart` $\rightarrow$ `Lifecycle::Idle`, attention `None`.
    - `UserPromptSubmit` $\rightarrow$ `Lifecycle::Working`, attention `Attention::None`.
    - `Notification` (`idle_prompt`) $\rightarrow$ `Lifecycle::Blocked`, attention `Attention::InputRequired`.
    - `PermissionRequest` $\rightarrow$ `Lifecycle::Blocked`, attention `Attention::PermissionRequired`.
    - `Stop` $\rightarrow$ `Lifecycle::Done`, attention `Attention::Unread`.
  - Codex ([`crates/signaltty-agent/src/adapters/codex.rs:26-70`](file:///home/simone/code/signaltty/crates/signaltty-agent/src/adapters/codex.rs#L26-L70)):
    - `SessionStart` $\rightarrow$ `Lifecycle::Idle`, attention `None`.
    - `UserPromptSubmit` $\rightarrow$ `Lifecycle::Working`, attention `Attention::None`.
    - `PermissionRequest` $\rightarrow$ `Lifecycle::Blocked`, attention `Attention::PermissionRequired`.
    - `Stop` / `agent-turn-complete` $\rightarrow$ `Lifecycle::Done`, attention `Attention::Unread`.
- **Location of "Ready for Input"**: There is no dedicated `Ready` state. A pane is ready for user input when:
  1. Initial startup: `Lifecycle::Idle`.
  2. Turn completed: `Lifecycle::Done` (attention `Unread`).
  3. Interactive input query: `Lifecycle::Blocked` with `Attention::InputRequired`.

---

## 3. `wait`, Accepted Values, and the `after_baseline` Mechanism

### Accepted `--until` Values
- **Validation**: [`crates/signaltty-server/src/params.rs:351-364`](file:///home/simone/code/signaltty/crates/signaltty-server/src/params.rs#L351-L364) (`validate_until`):
  - **Attention States**: `"none"`, `"unread"`, `"input_required"`, `"permission_required"`, `"warning"`, `"error"`.
  - **Lifecycle States**: `"unknown"`, `"working"`, `"blocked"`, `"done"`, `"idle"`, `"failed"`, `"exited"`.
  - **Pseudo-States**:
    - `"seen"`: Evaluates to `pane.attention == Attention::None`.
    - `"attention_cleared"`: Alias for `"seen"`.
    - `"exited"`: Evaluates to `pane.live == LiveState::Exited { .. } || pane.lifecycle == Lifecycle::Exited`.
- **Parsing**: In [`crates/signaltty-cli/src/main.rs:134-142`](file:///home/simone/code/signaltty/crates/signaltty-cli/src/main.rs#L134-L142), `--until` accepts repeated or comma-separated tokens parsed into `Vec<String>`.

### The `after_baseline` Mechanism
Introduced in Feature 016 ([`crates/signaltty-server/src/store.rs:28-44, 168-180`](file:///home/simone/code/signaltty/crates/signaltty-server/src/store.rs#L28-L44)), `WaitBaseline` solves the race condition where an agent finishes before a client calls `wait`:
```rust
// crates/signaltty-server/src/store.rs:28-36
pub struct WaitBaseline {
    pub pane_id: String,
    pub process_instance: String,
    pub agent_session_id: Option<String>,
    pub session_generation: u64,
    pub lifecycle_seq: u64,
    pub attention_seq: u64,
}
```
- **Capturing Baseline**: Callers invoke `pane.get` ([`crates/signaltty-server/src/router.rs:866`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L866)), which returns both the pane and its current `wait_baseline` before submitting an instruction.
- **Wait Evaluation Loop**: In [`crates/signaltty-server/src/router.rs:1658-1685`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L1658-L1685) (`h_wait`):
  1. Identity Check: Verifies `after.process_instance == current.process_instance` and `after.session_generation == current.session_generation`. If the child process restarted or the agent session was replaced, returns `IDENTITY_CHANGED`.
  2. Monotonicity: Verifies `after.lifecycle_seq <= current.lifecycle_seq` and `after.attention_seq <= current.attention_seq`.
  3. Threshold Satisfaction:
     ```rust
     let transition = s.matching_transition(&p.pane_id, outcome).unwrap_or(0);
     let satisfied = match &p.after {
         Some(after) => transition > after.threshold(outcome),
         None => wait_satisfied(pane, outcome),
     };
     ```
     - For attention states and `"seen"` / `"attention_cleared"`, `threshold(outcome)` returns `after.attention_seq`.
     - For lifecycle states and `"exited"`, `threshold(outcome)` returns `after.lifecycle_seq`.
     - `Store::matching_transition` ([`crates/signaltty-server/src/store.rs:160-166`](file:///home/simone/code/signaltty/crates/signaltty-server/src/store.rs#L160-L166)) returns `progress.transitions[outcome]`.
     - Because `progress.transitions` records the exact global `seq` when each transition occurred, `transition > after.threshold(outcome)` is satisfied if and only if the event occurred **strictly after** the baseline was captured.

---

## 4. `status` Output and Blocked/Needs-You Pane Discovery

### What `status` Returns Today
- **IPC Protocol**: `server.status` ([`crates/signaltty-server/src/router.rs:228-245`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L228-L245)) returns:
  ```json
  {
    "version": "0.1.0",
    "protocol": "signaltty/1",
    "uptime_s": 124,
    "workspaces": 2,
    "tabs": 3,
    "panes": 5,
    "live_panes": 4,
    "capabilities": {
      "subscribe_replay_coverage": true,
      "wait_baseline": true,
      "wait_multiple_outcomes": true
    }
  }
  ```
- **CLI Presentation**: [`crates/signaltty-cli/src/main.rs:391-408`](file:///home/simone/code/signaltty/crates/signaltty-cli/src/main.rs#L391-L408) prints:
  `signaltty 0.1.0 (signaltty/1) up 124s — 2 workspaces, 3 tabs, 5 panes (4 live)`
- It does **not** return any list of panes, their lifecycle states, or attention statuses.

### Existing Means to List Blocked/Needs-You Panes
- **`focus.next_unread`**: [`crates/signaltty-server/src/router.rs:1700-1703`](file:///home/simone/code/signaltty/crates/signaltty-server/src/router.rs#L1700-L1703) calls `Store::next_unread` ([`crates/signaltty-server/src/store.rs:306-319`](file:///home/simone/code/signaltty/crates/signaltty-server/src/store.rs#L306-L319)):
  ```rust
  pub fn next_unread(&self) -> Option<String> {
      let mut panes: Vec<&Pane> = self.panes.values().filter(|p| p.attention.needs_human()).collect();
      panes.sort_by(|a, b| b.attention.severity().cmp(&a.attention.severity()).then(b.last_activity_at.cmp(&a.last_activity_at)));
      panes.first().map(|p| p.id.clone())
  }
  ```
  This returns at most **one** pane ID (`{"pane_id": "pane_..."}`), representing the single highest-severity item needing human attention.
- **Current Limitation**: There is **no method** to list all blocked panes or filter panes by attention in a single query. An orchestrator or client currently has to:
  1. Call `workspace.list`, iterate workspaces with `workspace.get`, and inspect every pane.
  2. Or maintain an active `subscribe` stream tracking `attention.created`, `attention.cleared`, and `agent.blocked` events.

---

## 5. Pane and Workspace Creation, Origins, and Environment Injection

### Creation Fields
- **`workspace.create`**:
  - Request ([`crates/signaltty-server/src/params.rs:85-88`](file:///home/simone/code/signaltty/crates/signaltty-server/src/params.rs#L85-L88)): `name: Option<String>`, `cwd: Option<String>`.
  - Workspace entity ([`crates/signaltty-core/src/model.rs:627-640`](file:///home/simone/code/signaltty/crates/signaltty-core/src/model.rs#L627-L640)): `id`, `name`, `handle`, `cwd`, `git: GitInfo`, `tabs: Vec<String>`, `active_tab_id: Option<String>`, `auto_resume: bool`, `created_at`, `updated_at`.
- **`pane.spawn`**:
  - Request ([`crates/signaltty-server/src/params.rs:189-200`](file:///home/simone/code/signaltty/crates/signaltty-server/src/params.rs#L189-L200)):
    `workspace_id: String`, `tab_id: Option<String>`, `cwd: Option<String>`, `argv: Vec<String>`, `env: HashMap<String, String>`, `cols: Option<u16>`, `rows: Option<u16>`, `agent_hint: Option<String>`.
  - Pane entity ([`crates/signaltty-core/src/model.rs:473-510`](file:///home/simone/code/signaltty/crates/signaltty-core/src/model.rs#L473-L510)):
    `id`, `workspace_id`, `tab_id`, `title`, `cwd`, `argv`, `pty_size`, `live`, `restore_state`, `agent`, `lifecycle`, `last_lifecycle`, `lifecycle_since`, `last_run_secs`, `attention`, `attention_since`, `last_message`, `pending_decision`, `created_at`, `last_activity_at`, `last_seen_at`.

### Spawn-Origin Information
- **No Parentage or Origin Fields**: `Pane` contains no `parent_pane_id`, `parent_id`, `caller_id`, or `task_id`.
- **No Labels / Tags**: Neither `Workspace` nor `Pane` supports user/orchestrator metadata labels or tags.
- **Overwritten Environment Context**: In [`crates/signaltty-server/src/pty.rs:191-192`](file:///home/simone/code/signaltty/crates/signaltty-server/src/pty.rs#L191-L192), the server sets:
  ```rust
  cmd.env("SIGNALTTY_PANE", &req.pane_id);
  cmd.env("SIGNALTTY_SOCKET", &req.socket_path);
  ```
  If an agent running inside an existing pane spawns a sub-pane via IPC, the child pane receives its *own* new pane ID in `SIGNALTTY_PANE`. The calling pane's identity is completely discarded by the server.

### Environment Variable Filtering and Injection
[`crates/signaltty-server/src/pty.rs:25-36, 172-195`](file:///home/simone/code/signaltty/crates/signaltty-server/src/pty.rs#L25-L36) defines strict filtering rules:
- **Server Environment**: Inherited by default, minus `ENV_BLOCK`: `LD_PRELOAD`, `LD_LIBRARY_PATH`, `SIGNALTTY_SOCKET`.
- **Client Overrides (`req.env`)**: Filtered against `ENV_BLOCK`. Allowed only if explicitly present in `ENV_ALLOW` (`TERM`, `COLORTERM`, `LANG`, `LC_ALL`, `LC_CTYPE`, `EDITOR`, `VISUAL`, `PAGER`, `SHELL`, `SIGNALTTY_PANE`) or prefixed by `ENV_PREFIX_ALLOW` (`SIGNALTTY_`, `CLAUDE_`, `CODEX_`, `OPENCODE_`, `CURSOR_`).
- Default `TERM` is set to `xterm-256color` if unset.

---

## 6. Worktrees, Git Context, and Diff Inspection

### Existing Git and Worktree Primitives
- **Worktree Management**: Defined in [`crates/signaltty-server/src/worktrees.rs`](file:///home/simone/code/signaltty/crates/signaltty-server/src/worktrees.rs):
  - `worktree.list` (L104-124): Parses `git worktree list --porcelain`, correlating paths to open workspaces.
  - `worktree.create` (L126-174): Executes `git worktree add -b <branch> <path>`, enters directory references to prevent races, and creates an associated workspace.
  - `worktree.open` (L176-210): Validates registered checkouts; if already open, reuses the existing workspace.
  - `worktree.remove` (L212-259): Refuses removal if the checkout is the main repository, dirty, locked/prunable, or has live panes; runs `git worktree remove` while deliberately preserving the git branch.
- **Git Context & Diff Primitives**:
  - `workspace.diff` ([`crates/signaltty-server/src/git.rs:48-80`](file:///home/simone/code/signaltty/crates/signaltty-server/src/git.rs#L48-L80)): Runs `git diff --numstat HEAD` and `git ls-files --others` for untracked files. Returns rolled-up `files`, `dirs`, `added`, `removed`.
  - `workspace.file_diff` ([`crates/signaltty-server/src/file_diff.rs:18-120`](file:///home/simone/code/signaltty/crates/signaltty-server/src/file_diff.rs#L18-L120)): Computes a unified diff for a single file relative to `HEAD` (or synthetic empty tree for untracked files), parsing hunks into `signaltty_core::diff::FileDiff`.
- **Existing Scripts**:
  - [`examples/workflows/fanout-review.sh`](file:///home/simone/code/signaltty/examples/workflows/fanout-review.sh): Demonstrates parallel orchestration using `signaltty new`, `pane split`, `wait --until done`, `pane read | tail -20`, and `notify`.
  - [`examples/plugins/fanout/fanout.sh`](file:///home/simone/code/signaltty/examples/plugins/fanout/fanout.sh): Plugin wrapper for fanout.

### Scope of Diff Analysis in Existing Code
As documented in ADR-0011 and Feature 014 ([`specs/014-file-diff-review/spec.md`](file:///home/simone/code/signaltty/specs/014-file-diff-review/spec.md)), **all diffs are strictly worktree-vs-HEAD**. Signaltty has no concept of a "turn diff", "task diff", or commit-range diff. Any orchestrator needing to verify task output against a specific starting point must record the starting `HEAD` SHA itself.

---

## 7. Events, Persistence, and Storage of New Entities

### Event Types Catalog
Defined in [`crates/signaltty-proto/src/lib.rs:205-244`](file:///home/simone/code/signaltty/crates/signaltty-proto/src/lib.rs#L205-L244) (`mod event`):
- **Workspace**: `workspace.created`, `workspace.updated`, `workspace.closed`.
- **Worktree**: `worktree.changed`.
- **Tab**: `tab.created`, `tab.updated`, `tab.closed`.
- **Pane**: `pane.created`, `pane.updated`, `pane.exited`, `pane.closed`, `pane.resized`.
- **PTY**: `pty.data`.
- **Agent Lifecycle**: `agent.working`, `agent.blocked`, `agent.done`, `agent.failed`, `agent.idle`, `agent.unknown`, `agent.exited`.
- **Decisions**: `decision.created`, `decision.answered`, `decision.cleared`.
- **Attention**: `attention.created`, `attention.updated`, `attention.cleared`.
- **Notifications**: `notification.created`.
- **Git & Server**: `git.branch_changed`, `server.will_shutdown`.

### Event Storage and Replay (Feature 016)
[`crates/signaltty-server/src/audit.rs:20-90`](file:///home/simone/code/signaltty/crates/signaltty-server/src/audit.rs#L20-L90):
- **Durable Sequence**: `EventSequence` maintains a lease-backed sequence counter in `event-sequence.json` allocated in chunks of 4,096.
- **Journal Storage**: `AuditLog` writes JSONL records to `audit.jsonl`, rotating to `audit.prev.jsonl` when exceeding `CAP_BYTES` (8 MB).
- **In-Memory Ring**: `Store::events` retains the last 1,024 events in memory.
- **Replay Mechanism**: `subscribe` (`from_seq`) combines on-disk historical audit logs with in-memory ring events. Replay coverage returns `status: "complete" | "history_lost" | "unavailable" | "cursor_ahead"`.

### Snapshot Schema and Migration Rules
[`crates/signaltty-server/src/persist.rs:20-29`](file:///home/simone/code/signaltty/crates/signaltty-server/src/persist.rs#L20-L29):
```rust
pub struct Snapshot {
    pub version: u32,
    pub saved_at: DateTime<Utc>,
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub panes: Vec<Pane>,
    pub notifications: Vec<Notification>,
}
```
- **Strict Deserialization Risk**: [`crates/signaltty-server/src/persist.rs:100-120`](file:///home/simone/code/signaltty/crates/signaltty-server/src/persist.rs#L100-L120) runs `serde_json::from_slice::<Snapshot>(&data)`. If any required field is missing or fails to deserialize, **the entire snapshot is considered corrupt**, backed up to `snapshot.corrupt-<timestamp>`, and discarded.
- **Migration Invariant**: Any new field added to `Snapshot` or child entities (`Pane`, `Workspace`) **must** include `#[serde(default)]` (and `#[serde(skip_serializing_if = "...")]` where appropriate) to guarantee seamless loading of existing snapshots.

### Placement of a New `Task` Entity
To introduce a top-level `Task` entity for Feature 018:
1. **Model**: Define `Task` in `crates/signaltty-core/src/model.rs`.
2. **Store**: Add `pub tasks: HashMap<String, Task>` in `Store` ([`crates/signaltty-server/src/store.rs:50-70`](file:///home/simone/code/signaltty/crates/signaltty-server/src/store.rs#L50-L70)), alongside task mutators emitting `task.created`, `task.updated`, `task.deleted`.
3. **Persistence**:
   - Add `#[serde(default)] pub tasks: Vec<Task>` to `Snapshot` in `crates/signaltty-server/src/persist.rs`.
   - In `persist::apply`, restore tasks into `Store::tasks`.
4. **Protocol**: Register `task.*` methods in `signaltty_proto::method::ALL` and `task.*` events in `signaltty_proto::event::ALL`.

---

## 8. Testing Infrastructure and Orchestration Verification

### Integration Testing Framework
- **Testkit**: [`crates/signaltty-testkit/src/lib.rs`](file:///home/simone/code/signaltty/crates/signaltty-testkit/src/lib.rs):
  - `TestServer::start()`: Creates a unique temporary directory (`signaltty-test-<pid>-<rand>`), starts `signaltty-server` with `--socket <temp>` and `--state-dir <temp>`, and waits for readiness on `server.status`. Automatically kills the server and removes directory artifacts on `Drop`.
  - `TestClient`: Establishes a Unix domain stream, serializes JSONL requests, correlates response IDs, and collects broadcast events into `pub events: Vec<Value>`.

### Mock / Fake Agent Existence
- **No Standalone Fake Agent Binary**: There is currently no standalone mock agent crate or binary.
- **Existing Test Seams**:
  1. Synthetic Hook Injection: Tests in [`crates/signaltty-server/tests/integration.rs:965-1095`](file:///home/simone/code/signaltty/crates/signaltty-server/tests/integration.rs#L965-L1095) call IPC `"hook-event"` directly (`SessionStart`, `UserPromptSubmit`, `Stop`, `PermissionRequest`), validating server reactions without any agent executable.
  2. Python/Shell Test Fixtures: [`crates/signaltty-server/tests/native_permissions.rs:40-60`](file:///home/simone/code/signaltty/crates/signaltty-server/tests/native_permissions.rs#L40-L60) (`provider_fixture`) writes a 15-line script acting as a provider agent that reads installed hooks and echoes responses.

### Simplest Deterministic End-to-End Orchestration Test Scenario
An end-to-end orchestration flow can run entirely in an integration test in `crates/signaltty-server/tests/` in under 100 ms:
1. `TestServer::start()` initializes an isolated daemon.
2. Spawns worker pane via `pane.spawn` running a simple echo script (e.g. `sh -c 'read line; echo "RESULT: $line"; exit 0'`).
3. Captures baseline: `c.call("pane.get", json!({"pane_id": id}))` $\rightarrow$ extract `wait_baseline`.
4. Dispatches input: `c.call("pane.input", json!({"pane_id": id, "data_b64": encode("task_payload\n")}))`.
5. Emits/waits for lifecycle transitions: Worker process finishes; client invokes `c.call("wait", json!({"pane_id": id, "until": "exited", "after": baseline}))`.
6. Inspects output: `c.call("pane.read", json!({"pane_id": id, "mode": "screen"}))` asserts `RESULT: task_payload`.
7. Cleanup: Pane closes or worktree removed.

---

## 9. Gaps and Constraints for Feature 018 (Orchestrator)

1. **PTY Read Distortion**: `pane.read` in `tail` mode does not interpret VT100 cursor controls, garbling full-screen TUI outputs; `screen` mode has zero scrollback lines.
2. **Missing Input Formatting / Bracketed Paste**: `pane.input` does not wrap multiline prompts in bracketed paste (`\x1b[200~` ... `\x1b[201~`) or translate `\n` to `\r`, which is required to submit interactive TUI prompts without premature line execution.
3. **No Explicit "Ready for Input" Lifecycle State**: The orchestrator must track a combination of `Lifecycle::Idle`, `Lifecycle::Done`, and `Lifecycle::Blocked` with `Attention::InputRequired`.
4. **Lack of Pane Querying & Filtering**: `server.status` provides only aggregate counts. `focus.next_unread` returns at most one pane. There is no query to retrieve all blocked panes, requiring a full crawl of `workspace.list` and `workspace.get`.
5. **No Parentage or Origin Metadata**: `Pane` has no `parent_pane_id`, `caller_id`, or `labels`. Spawning a sub-pane clobbers `$SIGNALTTY_PANE` with the child pane's ID, leaving no tracking of which agent spawned which worker.
6. **Diffs Are Limited to Worktree vs. HEAD**: All existing diff APIs compare against current repository `HEAD`. Orchestrating tasks will require capturing baseline commit SHAs to isolate per-task diffs.
7. **Strict Snapshot Deserialization**: Adding `Task` to `Snapshot` requires strict usage of `#[serde(default)]` to prevent invalidating existing state files during upgrades.

# Feature Specification: Self-Printing Schema

**Feature Branch**: `002-self-printing-schema`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "docs/14 directive 4 requires a versioned JSON socket API with a self-printing schema (cmux cli-contract discipline); the server has no introspection method and the CLI cannot list the contract"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Agent asks the server what it can do (Priority: P1)

An agent running inside a managed pane (directive 5: agents are API
clients) calls one method and learns the full contract — method names,
event names, error codes, protocol version — instead of guessing from
stale docs or probing method by method.

**Why this priority**: Directive 4 names "a self-printing schema" as
part of the socket/CLI contract discipline. It is also the cheapest
way to keep agents working across server upgrades: they read the
contract at runtime.

**Independent Test**: `signaltty schema` (or `server.schema` over the
socket) returns the contract; an integration test asserts every listed
method dispatches (never `UNKNOWN_METHOD`) and every dispatched method
is listed.

**Acceptance Scenarios**:

1. **Given** a running server, **When** a client calls `server.schema`
   with `{}`, **Then** it gets `{protocol, version, methods[],
   events[], codes[]}` with `methods` containing `server.status`,
   `pane.spawn`, `hook-event`, `wait`, `focus.next_unread`.
2. **Given** the schema result, **When** a client calls each listed
   method with `{}` (except `server.shutdown`, which would stop the
   fixture), **Then** no response carries `UNKNOWN_METHOD`.
3. **Given** `signaltty schema`, **When** run with and without `--json`,
   **Then** `--json` prints the raw `result` object and the default
   prints a human-readable listing of the same data.

---

### User Story 2 - Contract drift fails loudly (Priority: P2)

A developer adds a method to the router but forgets the schema list
(or vice versa); a test fails naming the drift, instead of agents
silently missing the capability.

**Why this priority**: A self-printing schema that can silently drift
from the router is worse than none — it teaches clients a lie. The
sync test is the load-bearing part of this feature.

**Independent Test**: The sync test in US1 doubles as this story's
test: delete one entry from either side and watch it fail.

**Acceptance Scenarios**:

1. **Given** the canonical method list in `signaltty-proto` and the
   router dispatch table, **When** the integration suite runs,
   **Then** a test proves every schema method dispatches and the
   schema response is built from the same constants the router
   matches on (single source of truth, not a copied list).

---

### Edge Cases

- `server.schema` takes no params; unknown fields in params are
  ignored per the forward-compat rule (like `server.status`).
- The schema call itself appears in `methods` (introspection is
  introspectable).
- `server.shutdown` is listed but excluded from the dispatch-probe
  loop (it would terminate the test server); its dispatch is already
  covered by existing shutdown tests.
- Event/code lists are informational; no dispatch equivalent exists,
  so they are asserted for non-emptiness and key members only.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The server MUST expose `server.schema` (no params)
  returning `{protocol, version, methods[], events[], codes[]}`.
- **FR-002**: The three lists MUST be built from canonical `ALL`
  slices in `signaltty-proto` (`method::ALL`, `event::ALL`,
  `code::ALL`) — the same constants the router matches on. No copied
  string lists anywhere.
- **FR-003**: The CLI MUST expose `signaltty schema` following the
  `status` command pattern (`emit(json, &result, human)`), where the
  human rendering lists methods, events, and codes.
- **FR-004**: `docs/08-ipc.md` MUST gain a `server.schema` row in the
  methods table (per AGENTS.md: new method → doc row + integration
  test).
- **FR-005**: An integration test MUST call `server.schema`, then call
  every listed method except `server.shutdown` with `{}`, and assert
  no error code equals `UNKNOWN_METHOD`.

### Key Entities

- **Schema**: `{protocol: "signaltty/1", version, methods: string[],
  events: string[], codes: string[]}` — versioned contract snapshot.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `signaltty schema --json | jq .methods` lists 30+
  methods including `server.schema` itself.
- **SC-002**: Removing any method from either the router or
  `method::ALL` breaks at least one test.
- **SC-003**: `cargo test --workspace`, clippy, and fmt gates pass.

## Assumptions

- Method/event/code names are already centralized as `&str` constants
  in `signaltty-proto`; adding `ALL` slices is additive and safe.
- Probing every method with `{}` is side-effect free except
  `server.shutdown` (excluded): mutating methods fail param decode
  (`BAD_PARAMS`) or id lookup (`NO_SUCH_*`) before touching state.
  `server.shutdown` with `{}` on an empty store *would* stop the
  server, hence the exclusion.
- CLI `schema` needs no new clap plumbing beyond one enum variant +
  one match arm (same shape as `Status`).

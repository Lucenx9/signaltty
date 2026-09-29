# Plan

## Grounding and architecture

Compared two grounded alternatives: A shares an OS integration library at the PTY launch boundary; B invokes the existing CLI installer through an asynchronous subprocess preflight and revalidates ownership afterward. B needs process deadlines, output parsing and an additional runtime packaging dependency. Select shared signaltty-integration library: one typed transaction engine, no subprocess protocol/timeouts or installer duplication. Narrow preparation to direct supported commands, because provisioning all discovered providers on shell launch exceeds the approved trigger.

ADR-0013 records OS boundary and policy. Installation has bounded per-file lock waiting and atomic same-directory replacement. Existing ownership locking around PTY publication remains intact. Setup notices are additive IPC results, native Adwaita toasts and CLI stderr. Core/adapters remain pure.

## TDD seams

Public CLI malformed-config preservation first (red), shared engine public operations for ownership/path/concurrency, real server IPC + PTY tests for launch ordering/failure/split/resume/state. No live model requests needed.

## Risks

Codex configuration changes require provider trust review; setup cannot promise active delivery. Direct command identification remains basename-based for unambiguous supported binary names, manifest aliases remain detection-only. Custom disabled/alternate config modes are reported explicitly instead of silently overridden. Non-cooperating external writers must not be knowingly overwritten; advisory locks serialize Signaltty writers only.

## Final interface and refinements

`Hooks::{install,uninstall,installed,file_for}` owns provider schemas and atomic
configuration transactions. `Hooks::from_env` discovers roots; `with_overrides`
normalizes launch-specific roots against cwd. Reports carry agent, file, changed
and an optional provider notice. Server preparation adds configured/disabled/error
status without preventing launch. Shared classifiers still detect kind purely.

AgentInfo.config_env retains only the three supported explicit directory paths,
not arbitrary env or credentials, across official resume and server restart.
Nonregular files are rejected using nonblocking open + descriptor metadata;
reads cap at 4 MiB. New files use 0600; existing permissions and final-file
symlinks are preserved. Owned legacy hooks require an exact generated command;
new managed hooks have a dedicated suffix. Foreign metadata and empty events
remain after uninstall.

## Architect phases

- [x] Ground: server execution, CLI installer, provider loading and trust.
- [x] Sketch: two structurally distinct full alternatives.
- [x] Agree: autonomous implementation authorized by the user.
- [x] Implement: shared transaction engine and direct-launch preparation.
- [x] Scrap assessment: no repeated architectural friction; preserve selected shape.

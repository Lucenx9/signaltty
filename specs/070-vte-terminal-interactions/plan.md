# Implementation plan: reliable terminal interactions

Branch: `t3/vte-terminal-improvements`. Spec: [spec.md](spec.md). Date: 2026-10-15.

## Summary and technical context

Rust 2021, minimum 1.92 and development 1.94. GTK4 4.18, libadwaita 1.7, VTE 0.80 through existing Rust bindings. All four changes stay in the GUI. No persisted data, server ownership or socket contract changes. Enable the existing binding's v0_76 feature for native VTE context menus if necessary; the reference runtime already requires VTE 0.80. Native tests use xdotool for keyboard input and ImageMagick's import for popover captures, declared in the development package manifest, display doctor and verification-runner fixture. The application gains no runtime dependency.

## Grounding and design

Grok 4.7 high traced the input, clipboard, snapshot and font paths. Independent architect candidates were Grok 4.7 high and GPT-5.6-Sol max. Their native experiments and the parent's probes found that reset with retained history duplicates visible rows, and a raw length-bearing C callback can receive a string already shortened at an embedded NUL. The safe synthesis is recorded in [ADR-0040](../../docs/adr/0040-pane-local-vte-interactions.md).

Use candidate A's existing JSON send path and terminal-local actions. Use candidate B's explicit exit from a stale alternate grid before the packed screen reconciliation. Keep existing `replace_screen` and `feed` names. Reject the new actor `PaneInput` layer, renames, server history replay and window-wide font state. Independent cross-judgment and the final design comparison are retained under ignored `target/vte-work/`.

## Data shape and interfaces

`PaneWidget` remains the public boundary. A private input adapter connects to VTE without converting bytes to Rust text. Its callback receives a slice valid for that signal invocation. It bounds the declared length by C string storage including the terminator and refuses irrecoverable truncation before any RPC. The existing `send` path base64-encodes accepted input. A weak pane reference and the existing live guard retain ownership behavior.

A private `TerminalCommand` enum represents Copy, Paste, Increase Text Size, Decrease Text Size and Reset Text Size. One terminal-local action group serves menu items and shortcuts. Copy enablement derives from selection; Paste enablement derives from live state. Clipboard reads are invalidated when the pane exits, so an asynchronous result cannot cross a resume boundary. Accepted paste text goes through VTE once and adds no Enter. Reserve plain Ctrl chords for child programs. Use Ctrl+Shift+C/V, Ctrl+Shift+plus/minus/0 and retain native existing font-size signals. Native Menu/Shift+F10 and right click use VTE's context-menu model, without a competing pointer gesture.

Text zoom is a bounded step value local to the pane, normal at zero. Derive its VTE font factor rather than changing the desktop font or line spacing. Reapply it during style updates; do not recreate terminals or add a resize path.

`replace_screen` builds one ordered buffer: leave alternate screen, reset origin/margins, erase each visible row with CUP and EL2, DECSTR, original snapshot. No direct VTE reset call or parser re-entry is needed. Keep actor offset handling unchanged. Verify queued live output and repeated snapshots before relying on this behavior.

## Work sequence and checkpoints

1. Planning and baseline gates precede the single source writer.
2. One implementation owner writes code in four vertical red/green slices: input, native controls and paste, screen replacement, text zoom. Each slice records its failing and passing behavioral checks before the next.
3. Independent report writers use separate ignored artifacts. The parent reviews every source change. No other agent writes implementation files.
4. Update touched docs and the verification map, inspect native light/dark renders, complete independent Standards and Spec reviews plus adversarial review of the unsafe boundary, then freeze source.
5. Stage final source, run full verification and fresh delivery proof, commit the identical content, push and publish a ready PR. No merge is authorized.

## Constitution check

Spec and clarification precede implementation. Existing seams are accepted by docs/15 and docs/18, so no repeated interview or issue publication is needed. Test first at the real VTE-to-actor, clipboard, screen/history and pane identity boundaries. GTK display tests are necessary for these native behaviors. Core remains pure, the server owns PTYs, and structural reconciliation remains unchanged. ADR-0040 records the native boundary decision. No constitution deviation.

## Verification and limits

Run targeted GTK tests individually under Xvfb and D-Bus, then `scripts/verify.sh full` on staged frozen source. Inspect native light/dark menu and text-scale renders. Use the existing actor reconnect regression and test canonical input modes, main/alternate history, repeated snapshots, actual shortcut matching, clipboard output, bracketed paste and exited-pane refusal. Xvfb does not certify real Wayland clipboard, IME or assistive technology. Native VTE's unrecoverable embedded-NUL limitation is explicit in the spec and ADR.

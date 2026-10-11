# Research

- Decision: retain OSC payload after `9;` as `4;<state>[;<value>]`, matching
  herdr manifests at 2563803. State 0..4; value omitted/empty or integer -1..100.
  Reserve all `4;` payloads and bare `4` even when invalid, rather than notifying.
  Sources: ConEmu ANSI OSC reference and herdr `src/detect/manifests/grok.toml`.
- Decision: latest evidence is scanner/PTY-local, ephemeral and never changes lifecycle
  directly. Alternatives: storing in Pane would pollute persistence/API; interpreting
  busy directly would bypass manifest scoping and hook precedence.
- Decision: extend matcher input explicitly rather than hidden global state.
  Absent progress never matches, even a catch-all expression.
- Decision: Grok rules use generic/binaries and existing launcher discovery.
  A new AgentKind/native hook installation is unnecessary for detection alone.
- Decision: preserve herdr priorities and phrase matching (case insensitive literals,
  case-sensitive regex). Translate nested alternatives into equivalent regex gates.
  The existing visible screen is the evidence boundary; no transcript parsing.

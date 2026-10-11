# Plan

Add serde-defaulted WorkspaceNaming enum and optional workspace.create auto_name.
Store methods pair naming transitions with workspace.updated events. Background
server module handles generation from accepted prompt hooks and bounded providers.
Native dialog switch chooses automatic/manual mode. Sidebar moves status to a
separate row, keeping title width available. No new dependency or IPC method.
Record ADR-0031, model/IPC/agent docs and feature verification map.

Design alternatives: current title/status share width; title plus status/activity
row (selected); compact title-only rail (rejected: hides attention/context).
Chosen layout adds one compact status row rather than mixing a long AI title and
attention pill. Quiet shells omit the activity headline as before.

Optional task chips have their own line below the headline/status to preserve
attention readability with large fonts. Ordinary rows retain their compact height.

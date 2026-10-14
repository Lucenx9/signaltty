# Design sources and current evidence

Primary guidance inspected 2026-10-14: [GNOME tooltips](https://developer.gnome.org/hig/patterns/feedback/tooltips.html) says essential information must not rely on tooltip access. [Utility panes](https://developer.gnome.org/hig/patterns/containers/utility-panes.html) places subordinate review content beside the main view and adapts it to an overlay when narrow. This app already follows the latter behavior; preserve it.

[GtkTextTag documentation](https://docs.gtk.org/gtk4/class.TextTag.html) supports foreground and paragraph-background roles plus paragraph spacing in a single TextBuffer. This is sufficient to distinguish number prefixes, hunk headings and code while retaining native selection/copy.

Current source: changes.rs uses All/Turn as the HeaderBar title widget. Reader mixes number prefixes and code in one full-line semantic tag. Existing native tests prove literal content, copied numbered text, Back/focus/list identity, refreshed/stale responses, notices and narrow scrolling. Current capture baseline uses those tests; standalone test windows now mirror App's dark/high-contrast CSS classes so their app colors are comparable. No borrowed source/icon/asset is introduced. Requested design skills contribute proximity, visual hierarchy and predictable native feedback; Karpathy limits the implementation to these two surfaces.

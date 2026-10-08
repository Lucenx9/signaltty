# Board presentation research

Decision: use GTK ScrolledWindow with automatic horizontal scrolling and no outer vertical scrolling. The five column lists retain their vertical scroll policy. This corrects overflow without redesigning the board into another navigation model.

[GTK documentation](https://docs.gtk.org/gtk4/class.ScrolledWindow.html) describes native scrollbars, automatic Viewport wrapping and touch scrolling. [GNOME accessibility guidance](https://developer.gnome.org/hig/guidelines/accessibility.html) calls for keyboard access, text enlargement and high contrast.

The existing board combines `dim-label` with metadata opacity 0.65. A single semantic foreground reduction follows the sidebar's readable secondary-text approach. High contrast removes that reduction.

Alternatives: stacking columns changes the existing board model; a custom gesture adds complexity. Both are unnecessary for this correction.

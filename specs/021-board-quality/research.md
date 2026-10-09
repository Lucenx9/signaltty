# Board presentation research

Decision: use GTK ScrolledWindow with automatic horizontal scrolling and no outer vertical scrolling. The five column lists retain their vertical scroll policy. This corrects overflow without redesigning the board into another navigation model.

[GTK documentation](https://docs.gtk.org/gtk4/class.ScrolledWindow.html) describes native scrollbars, automatic Viewport wrapping and touch scrolling. [GNOME accessibility guidance](https://developer.gnome.org/hig/guidelines/accessibility.html) calls for keyboard access, text enlargement and high contrast.

The existing board sets both `dim-label` and application opacity 0.65 on the same widget. These declarations compete; they do not multiply. The correction removes redundant dim styling and raises the metadata foreground alpha to 0.8. High contrast removes that foreground reduction.

Alternatives: stacking columns changes the existing board model; a custom gesture adds complexity. Both are unnecessary for this correction.

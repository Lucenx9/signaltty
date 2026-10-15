# Implementation plan

## Design and review
Read docs 06/14/16/17/18/19 and constitution; apply requested design skills and karpathy-guidelines. Existing system sans and semantic canvas/view/text/border/accent roles remain; Signal dark examples canvas #090909, view #111113. Branch at weight 600, path 0.9em secondary currentColor at 0.78 with high-contrast full foreground. Task label wraps to two lines; full-foreground state sits beneath it, aligned with text rather than icon. Keep existing quiet sections, flat actions and PR/diff semantics. No extra section headings, animation or decorative panels.

Baseline Details has branch/path at the same size/weight and combines Task name · state in one middle-ellipsized label. T3 Code ThreadDetailsCard/ThreadDetailsPanel at 50647de0143e68fde051eb1a0ccf4fe63f9947b2 supplies compact contextual-card hierarchy; translate it to existing native rows. Grok 4.7 High completed an independent read-only audit. Do not move the centered shell identity. Do not mistake pane fixture IDs for production defects.

## Technical sequence
Existing details.rs owns widgets and fill. Add semantic CSS classes to existing branch/path labels, a task-state label under its row and a full title tooltip; keep decision data and IPC untouched. Native GTK logical alignment handles the new metadata line. An AdwClamp limits the card to 300px so long branch/path/title natural sizes cannot expand the popover beyond a 360px window. Extend existing Details test first for separate state/wrapping and stress long data at narrow/default/enlarged/DejaVu/RTL. Reproduce red, implement, capture/inspect actual renders and preserve exact clipboard/navigation checks. Evaluate Grok audit, obtain independent two-axis final review, retain selected evidence and Before/After/Why table. Freeze all source before full and delivery gate. Publish/link, inspect latest refs/reviews/CI and merge when authorized gates pass.

## Constitution and technical context
Rust 1.94 development/minimum 1.92, GTK4/libadwaita. GUI-only presentation with existing model and asynchronous generation guard. No new load-bearing choice, domain data or dependency. Native allocation requires GTK display tests; manual screen-reader/IME certification is not claimed.

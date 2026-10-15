# Design evidence

Current base 6c6234d native Details captures show equal branch/path type and a single middle-ellipsized Task name · state label. T3 Code ThreadDetailsCard/ThreadDetailsPanel at 50647de0143e68fde051eb1a0ccf4fe63f9947b2 organizes primary identity and quieter context without extra section headings. Translate the hierarchy through existing native labels and app semantic tokens.

Grok 4.7 High, delegated design audit gui-polish-grok47-design-20261015-r1, inspected real dark/light/approval/pane captures and source. It independently recommended Details hierarchy, and confirmed pane tools already have the intended reveal/grouping. Adopt branch emphasis and quieter context. Put task state beneath the title rather than trailing it, using the narrow-layout lesson from PR124 so metadata cannot squeeze identity. Reuse the bundled Board role for the task row.

The audit also suggested one-line ellipsis for approval options. Do not adopt it: those labels communicate permission scope and should remain readable without hovering. One cohesive Details PR satisfies the accepted 1–2 range; no second polish is needed merely to fill a count.

No new font, icon family, animation, dependency or architecture choice. Native typography/presentation roles and high-contrast behavior require real renders; no manual assistive-technology certification is inferred.

The long-data native test reproduced another existing issue: unrestricted natural label widths stretched the popup beyond its 360px parent. AdwClamp now caps the preferred card width at 300px while GTK retains child minimum sizes. The stress matrix verifies actual content and action bounds rather than assuming ellipsis constrains the popup.

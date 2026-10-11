# Research — 2026-10-11

Primary sources reviewed:
- [GNOME typography](https://developer.gnome.org/hig/guidelines/typography.html): platform font, few relative styles; weight and positioning communicate hierarchy.
- [Geist typography](https://vercel.com/geist/typography): defined roles for labels and secondary copy; tabular counters.
- [T3 Code product screenshot](https://t3.codes/) and [source](https://github.com/pingdotgg/t3code): quiet selected surface, explicit identity/activity/context hierarchy. Local reference inspected at 740bda40c9, Sidebar.tsx around 1941–2121.
- [Zed panels](https://zed.dev/blog/new-panel-system): controls local to their pane, optional panels leave coding space. No new panel behavior is needed here.

Decision: retain native font/theme, unify sidebar geometry and remove redundant
Generated Working… copy. Preserve explicit messages and durations by provenance.
Rationale: current render repeats Working twice and outlines selection in addition
to a leading marker. Useful branch context should remain stable and recognizable.
Alternative: more badges, accent washes, web fonts or a toolkit replacement would
add visual competition and violate the native product direction.

Independent research reviewer checked baseline/reference renders and current
T3 sidebar source. Key constraints adopted: status sibling remains visible,
metadata image is decorative and hidden with empty/child metadata, minimum
200px sidebar and high-contrast focus are checked; real assistive-tech checks
remain unclaimed.

# Primary-source research, 2026-10-12

- [Apple Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars?changes=_2), official [article data](https://developer.apple.com/tutorials/data/design/human-interface-guidelines/toolbars.json): choose few frequent controls, recognizable symbols, logical groups, native components and consistent start/center/end roles. Translate hierarchy to Linux; do not introduce Liquid Glass or Apple-specific window conventions.
- [Apple design foundations, WWDC25](https://developer.apple.com/videos/play/wwdc2025/359/): hierarchy, typography and appearance-adaptive semantic roles communicate importance. Existing native color/system font remain.
- [GNOME UI icons](https://developer.gnome.org/hig/guidelines/ui-icons.html): monochrome16px symbolic assets, recognizable meanings, safe inversion and grid alignment. Preserve the existing lighter1.5px family rather than adopting2px everywhere; inspect native rendered weight. Metadata uses16px rather than14px resampling.
- [GNOME header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html): few contextual controls, blank draggable space, tooltips, functional grouping through spacing rather than linked backgrounds.
- [KDE icons](https://develop.kde.org/hig/icons/): choose symbols by their context, disambiguate similar visible actions and retain accessible labels. Qt-specific packaging advice does not override this GTK application's bundled original assets.

Alternatives rejected: putting an icon on every palette/file row adds repetition; a new icon library adds dependency/licensing work without useful behavior; bright toolbar accents compete with actual attention; moving settings out of the pinned footer worsens context; shrinking targets to fit is unacceptable. Use original vector artwork and existing native actions.

Apple Icons page requires JS and its article data was unavailable through the web tool; no unsupported claims are drawn from that page. Accessible official Toolbars article and WWDC transcript support the decisions.

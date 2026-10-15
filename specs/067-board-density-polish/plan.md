# Implementation plan

## Design and grounding
Read docs06/14/16/17/18/19 and constitution. Apply apple-design, emil-design-eng, frontend-design and karpathy-guidelines. Native system sans; title 600/0.9em, secondary 0.82em; current canvas/view/border/text tokens. Existing dark Signal canvas #090909 and view #111113 remain; no copied brand colors. Layout: title / full-width wrapped context / compact leading state. Completed state uses text only, urgency retains existing semantic fills. Board height 440px, stable for its lifetime, native scrolling handles overflow.

T3 Code inspected at 50647de0143e68fde051eb1a0ccf4fe63f9947b2, Sidebar.tsx and CommandPaletteResults.tsx. Translate its text hierarchy and compact surfaces, preserve GTK controls. GNOME header-bar guidance supports sparse controls and grouping: https://developer.gnome.org/hig/patterns/containers/header-bars.html . Baseline native board capture has a 600px content request and truncated metadata beside badges.

Rejected: dynamic resizing on each update would disturb scroll/focus. Dropping Completed entirely could hide the worker axis behind PR stage. Existing tokens and native controls fit this product; no new palette/font/dependency or shell-title relocation.

## Sequence and validation
Extend the existing native board test first and demonstrate red. Move the status below metadata and wrap context; quiet completed fill; request 440px. Run exact desktop/narrow/live tests, inspect captures. Retain selected evidence and Before/After/Why review table, independent Spec/Standards review. Freeze and stage all source before full; fresh delivery check binds exact inputs. Publish/link and merge after latest required CI/reviews. External run reports do not modify source.

## Constitution check
All principles satisfied; GUI-only presentation, no new model/IPC/dependency or load-bearing architecture decision. Native display assertions are necessary for text allocation. Manual desktop/AT-SPI/IME are not certified by Xvfb.

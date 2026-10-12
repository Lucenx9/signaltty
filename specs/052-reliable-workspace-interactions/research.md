# UX decisions

- [GNOME dialogs](https://developer.gnome.org/hig/patterns/feedback/dialogs.html): use an action dialog for a form that must remain present during async work. Alert responses dismiss by design. Undo cannot restore a stopped PTY, so running sessions get a Cancel-default confirmation. Exited sessions close directly.
- [GNOME keyboard](https://developer.gnome.org/hig/guidelines/keyboard.html): all controls reachable by keyboard, logical focus, existing shortcuts preserved. Terminal Tab input remains untouched.
- [Nielsen heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/): visible status, user control, error recovery and recognition guide pending/retry and labeled compact tools.
- Alternatives: reopening an AlertDialog loses interaction continuity; toast-only errors lose draft recovery; global confirmations for already-exited terminals create avoidable interruption; keeping all header buttons at 360px crowds breadcrumb and terminal content.
- Known limits: no retry on ambiguous transport failure without known created id, no automatic retry, no undo for terminated processes, no claim of desktop assistive-technology certification from Xvfb.

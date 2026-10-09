# Specification: Explain the completed-task subset
**Branch**: t3/board-done-subset | **Created**: 2026-10-08
**Input**: Continue UI/UX quality with 2–3 further PRs, all brought to merge.
## User story 1 (P1)
A user reads Done · 25 but sees only 20 cards. Explain that these are the latest 20 of the total without implying deletion or completeness.
Acceptance: 25 finished tasks show total 25, exactly the newest 20 cards and “Showing latest 20 of 25”. At 20 or fewer no redundant subset notice appears. Empty board feedback remains unchanged.
## Requirements
Keep existing task derivation, order, cap and activation. Notice is native text, wraps at narrow sizes, remains readable in light/dark and enlarged text. No new IPC, storage, dependency or motion.
## Success criteria
A native production-board red regression proves missing subset feedback; after fix it checks total, visible count/order, notice absence for untruncated history and native render bounds. Full verification and independent review pass before merge.
## Clarifications
This is presentation of existing data, not a pagination feature. Earlier board attention/refresh issues are separate PRs. No unresolved requirements or constitution deviations.

# Plan

Before implementation, retain the red real-PTY close sequence and compare two cancellation designs.
Independent Sonnet design review was attempted but rate-limited; record that
limit and obtain a final review when a provider is available. A timeout
around a still-blocking syscall cannot release its fd/thread and is insufficient.

Prefer bounded native Linux I/O hidden in the PTY ownership module, with original
writer binding, cancellation on lifecycle changes and typed accepted-byte
evidence. Account for descriptor clones sharing file status flags and portable
writer destruction performing EOF I/O. Keep core toolkit/OS-free. Record the
load-bearing I/O choice in an ADR and the wire details in docs/08.

Implement the smallest selected design after red regression; run focused
input/submit/decision/replay tests and full verification, obtain final independent
review and merge only after exact-head CI passes. Desktop/provider sessions are
outside this server change's proof.

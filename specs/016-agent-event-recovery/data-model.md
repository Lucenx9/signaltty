# Data model

Domain Pane and persistence version 1 remain unchanged. Runtime progress belongs to
Store: process instance, lifecycle transition sequence and attention transition
sequence. Pane creation starts a process instance; resume replaces it; exit retains
it so an exit wait can complete. Pane removal deletes runtime progress.

A serializable WaitBaseline contains pane_id, process_instance, agent_session_id,
session_generation, lifecycle_seq and attention_seq. A baseline is valid only for the same pane/process.
A missing session may learn its first identity; a known session cannot change.
Each axis advances only on its own semantic transition, never on repeated hooks.
Runtime progress also retains the last transition per recognized outcome, so brief
matching states remain observable after the pane moves on.

Durable sequence reservations allocate scalar event numbers before issuance. Numeric
holes are valid and have no relationship to missing state history. Retention evidence
tracks discarded state events, independent of PTY numbers and subscription filters.
Replay metadata reports status, requested_after, retained_after, through and returned.

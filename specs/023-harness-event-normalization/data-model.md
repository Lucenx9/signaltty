# Data model

No new persisted entities. Provider events normalize to the existing AdapterEvent
payload (session_id, status or message). Error events use existing Failed/Error
transitions and notification drafts; resume uses the existing session identity.

Answering the current pending decision now transitions an existing Blocked pane
to Working via Store::set_lifecycle, paired with agent.working. Stale answers and
non-answer clears preserve lifecycle; no new state or schema is introduced.

# Reconciliation finding

Grok 4.7 High on the direct provider identified that sync_decision_bar caches only Decision.id while decision_render derives options from answerability and option data. The current model explicitly requires buttons iff answerable. Reconnect may retain the ID and remove answerability.

Decision: retain the existing Decision snapshot and compare the three fields defining button callbacks and labels. Comparing the entire Decision would rebuild for unrelated prompt or timestamp changes. Introducing another cache-key type duplicates an existing small domain shape.

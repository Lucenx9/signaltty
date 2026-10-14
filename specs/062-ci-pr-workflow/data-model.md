# Existing workflow concepts

A workflow event has event_name, PRnumber if applicable and full Gitref otherwise.
Concurrency identity is workflow+event+PRnumber/ref. Run jobs retain GitHub's
status/conclusion and tested revision; a cancelled older run is not successful
proof and does not erase a passing replacement at the latest revision.
Local evidence remains owned by verify.py/verification_evidence.py. No new
persisted state or application model is introduced.

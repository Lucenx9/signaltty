# Workflow data model

Feature identity is the complete specs directory slug. Canonical catalog entries
contain lifecycle (unreviewed, active, completed, superseded), enrolled boolean,
and optional superseded_by full slug. Task state remains in Markdown checkboxes.
Routing entries name task, first skills, conditional skills, documents and command
arrays. Repository observation combines artifact presence, open task IDs, required
FR identifiers, missing coverage and evidence status. Observation is never stored.
Evidence records the content/index digest at run start/end; each snapshot has
schema_version, root, revision and file count. Diagnosis is a stable code, message,
path and action. JSON CLI reports schema_version, command, ok and data/findings.

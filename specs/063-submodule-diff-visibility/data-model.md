# Data model

No schema change. Existing WorktreeDiff files/counts and FileDiff text hunks
represent gitlink changes. A Subproject commit removal/addition is a one-line
old/new commit comparison. Turn baseline and current trees retain gitlink OIDs.

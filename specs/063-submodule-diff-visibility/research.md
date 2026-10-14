# Diagnosis evidence

Command: `cargo test -p signaltty-server --test file_diff selected_submodule_diff_is_not_hidden_by_git_ignore_settings -- --exact`.

Before fix, repeated runs return content.kind=unchanged instead of text for an
advanced local submodule. Runtime after compilation: 0.36 to 1.19 seconds.
Changing only diff.ignoreSubmodules from all to none makes the fixture pass.
This rules out a non-advanced checkout and parser loss. The ranked hypotheses
were Git suppression, fixture checkout failure, and parser rejection.

Git's ignore setting suppresses changes before parsing. Pin the explicit diff
option rather than editing configuration; it also overrides per-submodule ignore
settings. Canonical clean/process conversion remains Git-owned. Existing specs
014/020/041 and ADR-0016/0029 define the comparisons and existing test seams.

Independent Spec review identified untracked-only submodule dirt as extra scope
with `none`. The Git reference documents `diff.ignoreSubmodules=untracked` as the
ordinary default: https://git-scm.com/docs/git-diff. A fixture with an untracked
file inside the advanced submodule failed with the unexpected `-dirty` suffix
under `none`; pinning `untracked` preserves pointer changes and tracked dirt
without introducing untracked-only noise.

Final targeted regression passes in 0.68 seconds after compilation. Committing
the pointer makes HEAD unchanged while the recorded task base still shows old/new
commits. Independent Standards review found no violations. Spec review's one
scope finding was fixed and its final round found no unresolved findings.

## Hosted Git 2.55 failure and correction

PR run 38588522653 used Git 2.55.0 and failed the gitmodules/turn assertion.
A locally built upstream v2.55.0 reproduces that exact failure in 0.36 seconds;
system Git 2.47.3 passes. A minimal independent Git snapshot probe confirms
that `git add --all` and plain explicit add skip .gitmodules ignore=all gitlinks
on 2.55, retaining the old OID. Tree-to-tree diff still displays distinct OIDs,
so diff suppression and baseline replacement were ruled out.

Git's force option works only with explicit submodule paths for this case.
Snapshot capture now enumerates tracked gitlinks from its private index and
force-adds just those literal NUL-delimited paths before writing its tree.
No configuration or user index changes. Untracked ignored repository files
remain excluded. A deinitialized submodule retains its recorded pointer.

The strengthened IPC regression includes module[1] as a literal path, unstaged
and explicitly force-staged gitlinks, and ignored repository content. It passes
on both Git 2.55.0 and 2.47.3 in under one second after compilation.

Both independent review axes found no unresolved findings in the CI recovery
patch. The capture remains in turns.rs and uses the existing private index,
without a Git-version branch or new architecture decision.

# Design research and synthesis

## Decision: Typed file diff, native full-width reader

The architect arena compared two independent packages: a persistent master/detail dialog with shared typed hunks and a navigation reader with raw unified text. The cross-judge recommended the typed-hunk package as the base and navigation as the graft. Both candidates converged after reading the clarified spec: accurate old/new numbers belong to a single pure parser, and a full-width reader avoids a second narrow-layout interaction model.

The rubric covered native keyboard/narrow readability, API depth, truthful HEAD/edge semantics, bounded concurrency/path safety, and verification. The chosen synthesis retains a single deep server operation and a single dialog owner. Raw-patch IPC would make the GUI parse hunk grammar; embedding all patches in the summary would turn a cheap read into a repository-wide content request. A permanently split pane adds adaptive layout/focus state. These alternatives are rejected for this increment.

## Decision: Fixed bounds and explicit reads

Use 512 KiB preview bytes, 10,000 lines, bounded stderr, eight seconds total backend and ten seconds GUI. Read on activation/refresh only, via dedicated IPC connections. Text truncation retains complete lines and is explicit; unsupported content gets a notice. These limits keep JSONL below the 16 MiB transport ceiling and prevent unlimited GTK buffer work. No configurable context/limit flags are needed.

## Decision: Git authority and literal paths

Run selected tracked-file comparisons at the repository root against HEAD or empty tree with three context lines and fixed no-color/no-ext-diff/no-textconv/no-renames semantics. Global literal-pathspec mode and exact NUL-separated membership prevent wildcard interpretation. Safe bounded untracked reads avoid dereferencing links/special files. Return typed hunk data rather than interpreting quoted filename headers. Resolve parent containment and account for no-follow acquisition; arbitrary paths are not a content API.

Primary references: [Git diff documentation](https://git-scm.com/docs/git-diff) for worktree/commit and external diff/textconv semantics; [Git command documentation](https://git-scm.com/docs/git) for literal pathspec mode. Existing pinned gtk4/libadwaita local bindings provide NavigationView/TextView, so no new UI dependency is needed.

Git's [tracked comparison](https://github.com/git/git/blob/v2.47.3/diff.c#L3966) applies [canonical conversion](https://github.com/git/git/blob/v2.47.3/convert.c#L1348), including locally configured clean/process filters, before comparing with HEAD. Preserve this trusted Git behavior; disabling it would change canonical content. Signaltty itself performs no staging/editing/Store mutations, while configured helpers can have their own effects. The process-group deadline applies to those helpers too. A private byte snapshot comparison was rejected because it would duplicate Git's normalization/attribute policy. Regression fixtures establish that tracked Git diff does not dereference replaced parent/leaf symlinks; untracked reads use no-follow descriptors.

Inherited `GIT_DIFF_OPTS` overrides `--unified=3`, so the selected-file process removes it. A public IPC context assertion fails under `GIT_DIFF_OPTS=--unified=0` before the fix and passes afterwards. Native TextTag colors come from rooted probes using current libadwaita CSS variables, not theme-independent GTK compatibility names.

## Clarification coverage

Functional scope, entities, interaction/loading/error states, latency/bounds, Git dependency, edge behavior and completion criteria are clear. Desktop accessibility follows native controls and redundant signs; localization follows existing English UI. External services, auth, compliance, persistent migration and background polling do not apply. No additional user answer is needed to implement the accepted scope.

## Verification

Design screening found no pass-through services, persisted selection cache, new provider, or duplicated patch parser. Code verification proceeds at the parser, real IPC, actor concurrency and actual GTK control seams. See tasks.md and quickstart.md.

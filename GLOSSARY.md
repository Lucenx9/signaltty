# signaltty

A native workspace for parallel coding agents. Projects organize sessions without owning their running processes.

## Language

**Project**: A repository or folder whose workspaces belong together. The main repository and its linked checkouts are one project.
_Avoid_: Workspace when referring to the grouping.

**Workspace**: A session containing tabs and terminal panes for one task or working context. Several workspaces can belong to a project.
_Avoid_: Project when referring to the session.

**Project scope**: The project whose workspaces the sidebar shows, or all projects. It does not change the terminal the user is currently viewing.

**Task child**: A workspace started for a worker by another workspace's agent. Its task ancestry and project membership are separate relationships.

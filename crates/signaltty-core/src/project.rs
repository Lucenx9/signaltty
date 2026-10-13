//! Repository/folder identity and client project scope. Pure data, no I/O.

use crate::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(directory: &str) -> Self {
        Self(
            Path::new(directory)
                .components()
                .collect::<std::path::PathBuf>()
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub fn directory(&self) -> &str {
        &self.0
    }

    pub fn name(&self) -> &str {
        Path::new(&self.0)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "project", rename_all = "snake_case")]
pub enum ProjectScope {
    #[default]
    All,
    Project(ProjectId),
}

impl ProjectScope {
    pub fn directory(&self) -> Option<&str> {
        match self {
            Self::All => None,
            Self::Project(id) => Some(id.directory()),
        }
    }

    pub fn includes(&self, id: &ProjectId) -> bool {
        match self {
            Self::All => true,
            Self::Project(selected) => selected == id,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CatalogAuthority {
    #[default]
    Retained,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub workspace_ids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectCatalog {
    projects: Vec<Project>,
    workspace_projects: HashMap<String, usize>,
    aliases: HashMap<ProjectId, HashSet<ProjectId>>,
}

impl ProjectCatalog {
    pub fn derive(workspaces: &[Workspace]) -> Self {
        let mut grouped: HashMap<ProjectId, Vec<String>> = HashMap::new();
        let mut aliases: HashMap<ProjectId, HashSet<ProjectId>> = HashMap::new();
        for ws in workspaces {
            let directory = ws
                .git
                .primary_root
                .as_deref()
                .or(ws.git.root.as_deref())
                .unwrap_or(&ws.cwd);
            let id = ProjectId::new(directory);
            grouped.entry(id.clone()).or_default().push(ws.id.clone());
            for alias in [Some(ws.cwd.as_str()), ws.git.root.as_deref()]
                .into_iter()
                .flatten()
            {
                aliases
                    .entry(ProjectId::new(alias))
                    .or_default()
                    .insert(id.clone());
            }
        }
        let mut projects: Vec<_> = grouped
            .into_iter()
            .map(|(id, workspace_ids)| Project {
                name: id.name().to_owned(),
                id,
                workspace_ids,
            })
            .collect();
        projects.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.directory().cmp(b.id.directory()))
        });
        let workspace_projects = projects
            .iter()
            .enumerate()
            .flat_map(|(index, project)| {
                project
                    .workspace_ids
                    .iter()
                    .map(move |id| (id.clone(), index))
            })
            .collect();
        Self {
            projects,
            workspace_projects,
            aliases,
        }
    }

    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    pub fn project_for_workspace(&self, id: &str) -> Option<&Project> {
        self.workspace_projects
            .get(id)
            .map(|index| &self.projects[*index])
    }

    pub fn contains(&self, id: &ProjectId) -> bool {
        self.projects.iter().any(|project| &project.id == id)
    }

    pub fn includes_workspace(&self, scope: &ProjectScope, id: &str) -> bool {
        matches!(scope, ProjectScope::All)
            || self
                .project_for_workspace(id)
                .is_some_and(|project| scope.includes(&project.id))
    }

    pub fn reconcile_scope(
        &self,
        previous: &Self,
        selected: &ProjectScope,
        authority: CatalogAuthority,
    ) -> ProjectScope {
        let ProjectScope::Project(id) = selected else {
            return ProjectScope::All;
        };
        if self.contains(id) {
            return selected.clone();
        }
        if let Some(aliases) = self.aliases.get(id).filter(|aliases| aliases.len() == 1) {
            return ProjectScope::Project(aliases.iter().next().unwrap().clone());
        }
        let surviving: HashSet<_> = previous
            .projects
            .iter()
            .find(|project| &project.id == id)
            .into_iter()
            .flat_map(|project| &project.workspace_ids)
            .filter_map(|workspace| {
                self.project_for_workspace(workspace)
                    .map(|project| project.id.clone())
            })
            .collect();
        if surviving.len() == 1 {
            return ProjectScope::Project(surviving.into_iter().next().unwrap());
        }
        match authority {
            CatalogAuthority::Retained => selected.clone(),
            CatalogAuthority::Complete => ProjectScope::All,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;
    use serde_json::json;

    fn ws(id: &str, cwd: &str, checkout: Option<&str>, primary: Option<&str>) -> Workspace {
        serde_json::from_value(json!({"id":id,"name":id,"cwd":cwd,
            "git":{"root":checkout,"primary_root":primary},"tabs":[],
            "created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"}))
        .unwrap()
    }

    #[test]
    fn main_nested_and_linked_checkouts_share_a_project() {
        let catalog = ProjectCatalog::derive(&[
            ws("main", "/repo", Some("/repo"), Some("/repo")),
            ws("nested", "/repo/src", Some("/repo"), Some("/repo")),
            ws(
                "linked",
                "/elsewhere/task",
                Some("/elsewhere/task"),
                Some("/repo"),
            ),
        ]);
        assert_eq!(catalog.projects().len(), 1);
        assert_eq!(catalog.projects()[0].id.directory(), "/repo");
        assert_eq!(catalog.projects()[0].workspace_ids.len(), 3);
    }

    #[test]
    fn identical_names_do_not_merge_folders_or_repositories() {
        let catalog = ProjectCatalog::derive(&[
            ws("a", "/one/api", Some("/one/api"), Some("/one/api")),
            ws("b", "/two/api", Some("/two/api"), Some("/two/api")),
            ws("c", "/notes", None, None),
            ws("d", "/notes/", None, None),
        ]);
        assert_eq!(catalog.projects().len(), 3);
        assert_eq!(
            catalog.project_for_workspace("c").unwrap().id,
            catalog.project_for_workspace("d").unwrap().id
        );
        assert_ne!(
            catalog.project_for_workspace("a").unwrap().id,
            catalog.project_for_workspace("b").unwrap().id
        );
    }

    #[test]
    fn only_complete_catalogs_can_discard_a_persisted_scope() {
        let previous = ProjectCatalog::derive(&[ws("a", "/repo", None, None)]);
        let selected = ProjectScope::Project(ProjectId::new("/repo"));
        let empty = ProjectCatalog::default();
        assert_eq!(
            empty.reconcile_scope(&previous, &selected, CatalogAuthority::Retained),
            selected
        );
        assert_eq!(
            empty.reconcile_scope(&previous, &selected, CatalogAuthority::Complete),
            ProjectScope::All
        );
    }

    #[test]
    fn legacy_checkout_scope_migrates_even_on_first_complete_catalog() {
        let canonical =
            ProjectCatalog::derive(&[ws("worker", "/linked/src", Some("/linked"), Some("/repo"))]);
        let selected = ProjectScope::Project(ProjectId::new("/linked"));
        assert_eq!(
            canonical.reconcile_scope(
                &ProjectCatalog::default(),
                &selected,
                CatalogAuthority::Complete
            ),
            ProjectScope::Project(ProjectId::new("/repo"))
        );
        let previous = ProjectCatalog::derive(&[ws("worker", "/linked", Some("/linked"), None)]);
        assert_eq!(
            canonical.reconcile_scope(&previous, &selected, CatalogAuthority::Retained),
            ProjectScope::Project(ProjectId::new("/repo"))
        );
    }

    #[test]
    fn initializing_git_does_not_change_a_folder_project_identity() {
        let previous = ProjectCatalog::derive(&[ws("a", "/repo", None, None)]);
        let next = ProjectCatalog::derive(&[ws("a", "/repo", Some("/repo"), Some("/repo"))]);
        assert_eq!(previous.projects()[0].id, next.projects()[0].id);
        assert_eq!(next.project_for_workspace("a").unwrap().name, "repo");
    }
}

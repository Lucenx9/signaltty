//! Task stages derived from persisted facts and an explicit observation time.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DispositionOutcome, PrChecks, PrMergeState, PrMergeability, PrReview, PrState, Task, TaskPr,
    TaskState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStage {
    Working,
    NeedsYou,
    InReview,
    ReadyToMerge,
    Done,
}

impl TaskStage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::NeedsYou => "Needs you",
            Self::InReview => "In review",
            Self::ReadyToMerge => "Ready to merge",
            Self::Done => "Done",
        }
    }
}

impl TaskPr {
    pub fn observation_fresh(&self, now: DateTime<Utc>) -> bool {
        self.refresh_error.is_none()
            && self.checked_at.is_some_and(|at| {
                let age = now.signed_duration_since(at);
                age >= Duration::zero() && age < Duration::minutes(5)
            })
    }

    pub fn ready_to_merge(&self, now: DateTime<Utc>) -> bool {
        self.state == PrState::Open
            && self.observation_fresh(now)
            && self
                .head_sha
                .as_ref()
                .is_some_and(|sha| !sha.trim().is_empty())
            && self.draft == Some(false)
            && self.mergeability == PrMergeability::Mergeable
            && self.merge_state == PrMergeState::Clean
            && matches!(self.checks, PrChecks::Passing | PrChecks::None)
            && matches!(self.review, PrReview::Approved | PrReview::None)
    }
}

pub fn task_stage(task: &Task, now: DateTime<Utc>) -> TaskStage {
    if matches!(
        task.disposition.outcome,
        DispositionOutcome::Merged | DispositionOutcome::Discarded
    ) || task.state == TaskState::Canceled
    {
        return TaskStage::Done;
    }
    if let Some(pr) = &task.pr {
        return match pr.state {
            PrState::Merged | PrState::Closed => TaskStage::Done,
            PrState::Open
                if pr.checks == PrChecks::Failing
                    || pr.review == PrReview::ChangesRequested
                    || pr.mergeability == PrMergeability::Conflicting =>
            {
                TaskStage::NeedsYou
            }
            PrState::Open if pr.ready_to_merge(now) => TaskStage::ReadyToMerge,
            PrState::Open => TaskStage::InReview,
        };
    }
    match task.state {
        TaskState::Completed => TaskStage::InReview,
        TaskState::InputRequired | TaskState::Failed | TaskState::Rejected => TaskStage::NeedsYou,
        _ => TaskStage::Working,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn task() -> Task {
        serde_json::from_value(json!({
            "id":"task_1", "context_id":"ctx_1", "label":"PR task",
            "contract":{"objective":"Implement a change"},
            "source_repo":"/repo", "worktree_path":"/worktree", "branch":"feature",
            "base_ref":"main", "base_sha":"abc", "state":"completed",
            "created_at":"2026-10-13T12:00:00Z", "updated_at":"2026-10-13T12:00:00Z",
            "pr":{"number":7, "url":"https://github.com/o/r/pull/7",
                "state":"open", "checks":"passing", "review":"approved",
                "head_sha":"abcdef", "draft":false, "mergeability":"mergeable",
                "merge_state":"clean", "checked_at":"2026-10-13T12:00:00Z"}
        }))
        .unwrap()
    }

    #[test]
    fn ready_requires_recent_complete_merge_facts() {
        let now = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        let task = task();
        assert_eq!(task_stage(&task, now), TaskStage::ReadyToMerge);
        for mutate in [
            |pr: &mut TaskPr| pr.draft = Some(true),
            |pr: &mut TaskPr| pr.draft = None,
            |pr: &mut TaskPr| pr.head_sha = None,
            |pr: &mut TaskPr| pr.head_sha = Some(" ".into()),
            |pr: &mut TaskPr| pr.mergeability = PrMergeability::Unknown,
            |pr: &mut TaskPr| pr.merge_state = PrMergeState::Unknown,
            |pr: &mut TaskPr| pr.merge_state = PrMergeState::Blocked,
            |pr: &mut TaskPr| pr.merge_state = PrMergeState::Behind,
            |pr: &mut TaskPr| pr.merge_state = PrMergeState::Unstable,
            |pr: &mut TaskPr| pr.checks = PrChecks::Pending,
            |pr: &mut TaskPr| pr.review = PrReview::ReviewRequired,
            |pr: &mut TaskPr| pr.checked_at = None,
            |pr: &mut TaskPr| pr.refresh_error = Some("authentication failed".into()),
        ] {
            let mut changed = task.clone();
            mutate(changed.pr.as_mut().unwrap());
            assert_eq!(
                task_stage(&changed, now),
                TaskStage::InReview,
                "{:?}",
                changed.pr
            );
        }
        assert_eq!(
            task_stage(&task, now + Duration::seconds(299)),
            TaskStage::ReadyToMerge
        );
        assert_eq!(
            task_stage(&task, now + Duration::seconds(300)),
            TaskStage::InReview
        );
        assert_eq!(
            task_stage(&task, now - Duration::seconds(1)),
            TaskStage::InReview
        );
    }

    #[test]
    fn incomplete_checks_withdraw_ready_to_merge() {
        let mut task = task();
        task.pr.as_mut().unwrap().checks =
            crate::rollup_status_checks(&[json!({"status": "COMPLETED", "conclusion": null})]);
        assert_eq!(task_stage(&task, task.created_at), TaskStage::InReview);
    }

    #[test]
    fn conflict_and_repository_checks_require_attention() {
        let mut task = task();
        let now = task.created_at;
        task.pr.as_mut().unwrap().mergeability = PrMergeability::Conflicting;
        assert_eq!(task_stage(&task, now), TaskStage::NeedsYou);
        task.pr.as_mut().unwrap().state = PrState::Merged;
        assert_eq!(task_stage(&task, now), TaskStage::Done);
        task.pr.as_mut().unwrap().state = PrState::Open;
        task.disposition.outcome = DispositionOutcome::Discarded;
        assert_eq!(task_stage(&task, now), TaskStage::Done);
    }

    #[test]
    fn legacy_pr_facts_load_without_claiming_readiness() {
        let mut value = serde_json::to_value(task()).unwrap();
        value["pr"] = json!({"number":7, "url":"https://github.com/o/r/pull/7",
            "checks":"passing", "review":"approved", "checked_at":"2026-10-13T12:00:00Z"});
        let task: Task = serde_json::from_value(value).unwrap();
        assert_eq!(task_stage(&task, task.created_at), TaskStage::InReview);
        let saved = serde_json::to_value(&task).unwrap();
        assert!(saved.get("stage").is_none());
    }
}

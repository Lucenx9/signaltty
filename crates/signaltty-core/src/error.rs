use thiserror::Error;

use crate::state::TaskState;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unknown id prefix in '{0}'")]
    BadId(String),
    #[error("invalid layout: {0}")]
    BadLayout(String),
    #[error("invalid size: {0}")]
    BadSize(String),
    #[error("invalid task transition from {0:?} to {1:?}")]
    InvalidTaskTransition(TaskState, TaskState),
    #[error("invalid contract: {0}")]
    InvalidContract(String),
    #[error("invalid task result: {0}")]
    InvalidTaskResult(String),
}

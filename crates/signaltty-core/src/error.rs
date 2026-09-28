use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unknown id prefix in '{0}'")]
    BadId(String),
    #[error("invalid layout: {0}")]
    BadLayout(String),
    #[error("invalid size: {0}")]
    BadSize(String),
}

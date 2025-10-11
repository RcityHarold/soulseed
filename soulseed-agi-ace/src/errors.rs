use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum AceError {
    #[error("authorization denied: {0}")]
    AuthDenied(String),
    #[error("quota depleted: {0}")]
    Quota(String),
    #[error("dfr error: {0}")]
    Dfr(String),
    #[error("schedule conflict: {0}")]
    ScheduleConflict(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("thin-waist failure: {0}")]
    ThinWaist(String),
    #[error("checkpoint error: {0}")]
    Checkpoint(String),
    #[error("outbox error: {0}")]
    Outbox(String),
    #[error("conversation assembler error: {0}")]
    Ca(String),
}

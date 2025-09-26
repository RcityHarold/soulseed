use thiserror::Error;

#[derive(Debug, Error)]
pub enum DfrError {
    #[error("authorization denied: {0}")]
    AuthDenied(String),
    #[error("quota limit: {0}")]
    Quota(String),
    #[error("invalid candidate: {0}")]
    InvalidCandidate(String),
    #[error("plan validation failed: {0}")]
    PlanValidation(String),
    #[error("no viable route")]
    NoRoute,
    #[error("thin waist failure: {0}")]
    ThinWaist(String),
}

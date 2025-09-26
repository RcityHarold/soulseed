use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Degradation {
    pub reason: String,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum GraphError {
    #[error("tenant mismatch or missing tenant context")]
    AuthForbidden,
    #[error("index plan is required but missing")]
    NoIndexPlan,
    #[error("scan without tenant or index is forbidden")]
    ScanForbidden,
    #[error("causal depth exceeded allowed maximum")]
    DepthExceeded,
    #[error("access restricted due to privacy policy")]
    PrivacyRestricted,
    #[error("storage conflict: unique constraint violation")]
    StorageConflict,
    #[error("idempotent conflict or replay detected")]
    IdempotentConflict,
    #[error("quota exceeded or rate limited")]
    QoSLimited,
    #[error("invalid query parameter: {0}")]
    InvalidQuery(&'static str),
}

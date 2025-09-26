use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContextError {
    #[error("privacy restricted: provenance required for restricted context")]
    PrivacyRestricted,
    #[error("pointer invalid: {0}")]
    PointerInvalid(String),
    #[error("quality gate failed for {su_id}: {reason}")]
    QualityGateFail { su_id: String, reason: String },
    #[error("planner failure: {0}")]
    PlannerFailure(String),
    #[error("anchor mismatch on {field}")]
    AnchorMismatch { field: &'static str },
    #[error("manifest invariant violated: {0}")]
    ManifestInvariant(String),
    #[error("delta mismatch: {0}")]
    DeltaMismatch(String),
    #[error("environment assembly failed: {0}")]
    EnvAssembly(String),
}

#[derive(Debug, Clone)]
pub struct QualityFailure {
    pub su_id: String,
    pub level: crate::types::Level,
    pub reason: String,
}

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("invalid field: {0}")]
    InvalidField(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolError {
    #[error("privacy restricted")]
    PrivacyRestricted,
    #[error("rate limited")]
    RateLimited,
    #[error("timeout")]
    Timeout,
    #[error("internal tool error: {0}")]
    Internal(String),
    #[error("schema invalid: {0}")]
    SchemaInvalid(String),
    #[error("auth forbidden")]
    AuthForbidden,
    #[error("budget exceeded")]
    BudgetExceeded,
    #[error("thin waist error: {0}")]
    ThinWaist(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlannerError {
    #[error("candidate list empty")]
    NoCandidates,
    #[error("anchor mismatch")]
    AnchorMismatch,
    #[error("config invalid: {0}")]
    ConfigInvalid(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrchestratorError {
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("all candidates failed")]
    AllFailed,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EngineError {
    #[error("config invalid: {0}")]
    ConfigInvalid(String),
    #[error("planner error: {0}")]
    Planner(#[from] PlannerError),
    #[error("orchestrator error: {0}")]
    Orchestrator(#[from] OrchestratorError),
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),
}

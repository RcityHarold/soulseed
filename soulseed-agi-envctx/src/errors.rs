use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvCtxError {
    #[error("missing required field: {0}")]
    Missing(&'static str),
    #[error("privacy restriction: {0}")]
    Privacy(&'static str),
    #[error("source unavailable: {endpoint} ({reason})")]
    SourceUnavailable {
        endpoint: &'static str,
        reason: String,
    },
    #[error("digest calculation failed: {0}")]
    Digest(String),
}

pub type Result<T> = std::result::Result<T, EnvCtxError>;

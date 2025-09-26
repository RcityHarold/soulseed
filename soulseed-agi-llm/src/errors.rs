use thiserror::Error;

use crate::tw_client::ThinWaistError;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("thin-waist error: {0}")]
    ThinWaist(#[from] ThinWaistError),
    #[error("missing tool summary for degrade-aware finalize")]
    MissingToolSummary,
    #[error("final event requires session identifier")]
    MissingSessionId,
}

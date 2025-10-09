use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::Anchor;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolDef {
    pub tool_id: String,
    pub version: String,
    pub capability: Vec<String>,
    pub input_schema: Value,
    pub output_schema: Value,
    pub side_effect: bool,
    pub supports_stream: bool,
    pub risk_level: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TwError {
    #[error("thin waist failure: {0}")]
    Failure(String),
}

pub trait ThinWaistClient: Send + Sync {
    fn tools_list(
        &self,
        anchor: &Anchor,
        scene: &str,
        capabilities: &[String],
    ) -> Result<(Vec<ToolDef>, Option<String>), TwError>;
}

#[derive(Default)]
struct TwMockState {
    defs: Vec<ToolDef>,
    policy_digest: Option<String>,
}

#[derive(Default, Clone)]
pub struct TwClientMock {
    inner: Arc<Mutex<TwMockState>>,
}

impl TwClientMock {
    pub fn with_defs(defs: Vec<ToolDef>) -> Self {
        let state = TwMockState {
            defs,
            policy_digest: None,
        };
        Self {
            inner: Arc::new(Mutex::new(state)),
        }
    }

    pub fn set_policy_digest(&self, digest: Option<String>) {
        self.inner.lock().unwrap().policy_digest = digest;
    }
}

impl ThinWaistClient for TwClientMock {
    fn tools_list(
        &self,
        _anchor: &Anchor,
        _scene: &str,
        _capabilities: &[String],
    ) -> Result<(Vec<ToolDef>, Option<String>), TwError> {
        let state = self.inner.lock().unwrap();
        Ok((state.defs.clone(), state.policy_digest.clone()))
    }
}

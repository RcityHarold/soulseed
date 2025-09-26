use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use thiserror::Error;

use crate::dto::{LlmResult, ModelProfile, PromptBundle};
use soulseed_agi_tools::dto::{AccessClass, Anchor};

#[derive(Error, Debug, Clone)]
pub enum ThinWaistError {
    #[error("llm thin waist failure: {0}")]
    Failure(String),
}

pub trait ThinWaistClient: Send + Sync {
    fn select_model(
        &self,
        anchor: &Anchor,
        scene: &str,
        hints: &[String],
    ) -> Result<ModelProfile, ThinWaistError>;

    fn execute(
        &self,
        anchor: &Anchor,
        prompt: &PromptBundle,
        model: &ModelProfile,
    ) -> Result<LlmResult, ThinWaistError>;

    fn reconcile(&self, anchor: &Anchor, result: &LlmResult) -> Result<(), ThinWaistError>;
}

#[derive(Clone)]
pub struct SoulbaseThinWaistClient {
    base_url: String,
    api_key: String,
    http: Client,
}

impl SoulbaseThinWaistClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, ThinWaistError> {
        let http = Client::builder()
            .build()
            .map_err(|err| ThinWaistError::Failure(err.to_string()))?;
        Ok(Self {
            base_url: trim_trailing_slash(base_url.into()),
            api_key: api_key.into(),
            http,
        })
    }

    fn headers(&self) -> Result<HeaderMap, ThinWaistError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let token = format!("Bearer {}", self.api_key);
        let value = HeaderValue::from_str(&token)
            .map_err(|err| ThinWaistError::Failure(err.to_string()))?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }

    fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R, ThinWaistError>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let response = self
            .http
            .post(url)
            .headers(self.headers()?)
            .json(body)
            .send()
            .map_err(|err| ThinWaistError::Failure(err.to_string()))?;

        if !response.status().is_success() {
            return Err(ThinWaistError::Failure(format!(
                "soulbase_http:{}",
                response.status()
            )));
        }

        response
            .json::<R>()
            .map_err(|err| ThinWaistError::Failure(err.to_string()))
    }
}

fn trim_trailing_slash(mut base: String) -> String {
    while base.ends_with('/') {
        base.pop();
    }
    base
}

#[derive(Serialize)]
struct AnchorPayload<'a> {
    tenant_id: u64,
    envelope_id: String,
    config_snapshot_hash: &'a str,
    config_snapshot_version: u32,
    session_id: Option<u64>,
    sequence_number: Option<u64>,
    access_class: &'static str,
    schema_v: u16,
}

fn anchor_payload(anchor: &Anchor) -> AnchorPayload<'_> {
    AnchorPayload {
        tenant_id: anchor.tenant_id.into_inner(),
        envelope_id: anchor.envelope_id.to_string(),
        config_snapshot_hash: &anchor.config_snapshot_hash,
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id.map(|id| id.into_inner()),
        sequence_number: anchor.sequence_number,
        access_class: match anchor.access_class {
            AccessClass::Public => "public",
            AccessClass::Internal => "internal",
            AccessClass::Restricted => "restricted",
        },
        schema_v: anchor.schema_v,
    }
}

#[derive(Serialize)]
struct SelectModelRequest<'a> {
    anchor: AnchorPayload<'a>,
    scene: &'a str,
    hints: &'a [String],
}

#[derive(Deserialize)]
struct SelectModelResponse {
    model: ModelProfile,
}

#[derive(Serialize)]
struct ExecuteRequest<'a> {
    anchor: AnchorPayload<'a>,
    model: &'a ModelProfile,
    prompt: &'a PromptBundle,
}

#[derive(Deserialize)]
struct ExecuteResponse {
    result: LlmResult,
}

#[derive(Serialize)]
struct ReconcileRequest<'a> {
    anchor: AnchorPayload<'a>,
    result: &'a LlmResult,
}

impl ThinWaistClient for SoulbaseThinWaistClient {
    fn select_model(
        &self,
        anchor: &Anchor,
        scene: &str,
        hints: &[String],
    ) -> Result<ModelProfile, ThinWaistError> {
        let request = SelectModelRequest {
            anchor: anchor_payload(anchor),
            scene,
            hints,
        };
        let response: SelectModelResponse = self.post_json("v1/llm/select-model", &request)?;
        Ok(response.model)
    }

    fn execute(
        &self,
        anchor: &Anchor,
        prompt: &PromptBundle,
        model: &ModelProfile,
    ) -> Result<LlmResult, ThinWaistError> {
        let request = ExecuteRequest {
            anchor: anchor_payload(anchor),
            model,
            prompt,
        };
        let response: ExecuteResponse = self.post_json("v1/llm/execute", &request)?;
        Ok(response.result)
    }

    fn reconcile(&self, anchor: &Anchor, result: &LlmResult) -> Result<(), ThinWaistError> {
        let request = ReconcileRequest {
            anchor: anchor_payload(anchor),
            result,
        };
        let _: serde_json::Value = self.post_json("v1/llm/reconcile", &request)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct MockThinWaistClient {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    model: ModelProfile,
    results: VecDeque<Result<LlmResult, ThinWaistError>>,
    reconciles: Vec<Value>,
}

impl MockThinWaistClient {
    pub fn new(model: ModelProfile) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                model,
                results: VecDeque::new(),
                reconciles: Vec::new(),
            })),
        }
    }

    pub fn push_result(&self, result: Result<LlmResult, ThinWaistError>) {
        self.inner.lock().unwrap().results.push_back(result);
    }

    pub fn take_reconciles(&self) -> Vec<Value> {
        std::mem::take(&mut self.inner.lock().unwrap().reconciles)
    }
}

impl ThinWaistClient for MockThinWaistClient {
    fn select_model(
        &self,
        _anchor: &Anchor,
        _scene: &str,
        _hints: &[String],
    ) -> Result<ModelProfile, ThinWaistError> {
        Ok(self.inner.lock().unwrap().model.clone())
    }

    fn execute(
        &self,
        _anchor: &Anchor,
        prompt: &PromptBundle,
        model: &ModelProfile,
    ) -> Result<LlmResult, ThinWaistError> {
        let mut inner = self.inner.lock().unwrap();
        let next = inner.results.pop_front().unwrap_or_else(|| {
            Ok(LlmResult {
                completion: format!("Model {} fallback response", model.model_id),
                reasoning: Vec::new(),
                degradation_reason: None,
                indices_used: None,
                query_hash: None,
                usage: Default::default(),
            })
        });
        if let Ok(res) = &next {
            inner
                .reconciles
                .push(serde_json::to_value(prompt).unwrap_or_default());
            inner
                .reconciles
                .push(serde_json::json!({"result": res.completion}));
        }
        next
    }

    fn reconcile(&self, _anchor: &Anchor, result: &LlmResult) -> Result<(), ThinWaistError> {
        self.inner
            .lock()
            .unwrap()
            .reconciles
            .push(serde_json::json!({"completion": result.completion}));
        Ok(())
    }
}

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};

use crate::dto::{Anchor, ToolCallSpec, ToolResultSummary};
use crate::errors::ToolError;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrechargeDecision {
    pub decision: String,
    pub reservation: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TwExecuteResult {
    pub summary: ToolResultSummary,
    pub degradation_reason: Option<String>,
    pub indices_used: Option<Vec<String>>,
    pub query_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TwEvent {
    ToolCalled {
        tool_id: String,
        idem_key: String,
    },
    ToolResponded {
        tool_id: String,
        idem_key: String,
    },
    ToolFailed {
        tool_id: String,
        idem_key: String,
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct Subject {
    pub human_id: Option<crate::dto::HumanId>,
    pub ai_id: Option<crate::dto::AIId>,
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

    fn precharge(
        &self,
        anchor: &Anchor,
        subject: &Subject,
        tool_id: &str,
        idem_key: &str,
    ) -> Result<PrechargeDecision, TwError>;

    fn execute(
        &self,
        anchor: &Anchor,
        subject: &Subject,
        call: &ToolCallSpec,
    ) -> Result<TwExecuteResult, TwError>;

    fn reconcile(&self, anchor: &Anchor, idem_key: &str, outcome: &Value) -> Result<(), TwError>;

    fn emit_events(&self, events: &[TwEvent]) -> Result<(), TwError>;
}

#[derive(Clone)]
pub struct SoulbaseToolsClient {
    base_url: String,
    api_key: String,
    http: Client,
}

impl SoulbaseToolsClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, TwError> {
        let http = Client::builder()
            .build()
            .map_err(|err| TwError::Failure(err.to_string()))?;
        Ok(Self {
            base_url: trim_trailing_slash(base_url.into()),
            api_key: api_key.into(),
            http,
        })
    }

    fn headers(&self) -> Result<HeaderMap, TwError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let token = format!("Bearer {}", self.api_key);
        let value =
            HeaderValue::from_str(&token).map_err(|err| TwError::Failure(err.to_string()))?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }

    fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R, TwError>
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
            .map_err(|err| TwError::Failure(err.to_string()))?;

        if !response.status().is_success() {
            return Err(TwError::Failure(format!(
                "soulbase_http:{}",
                response.status()
            )));
        }

        response
            .json::<R>()
            .map_err(|err| TwError::Failure(err.to_string()))
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
            crate::dto::AccessClass::Public => "public",
            crate::dto::AccessClass::Internal => "internal",
            crate::dto::AccessClass::Restricted => "restricted",
        },
        schema_v: anchor.schema_v,
    }
}

#[derive(Serialize)]
struct SubjectPayload {
    human_id: Option<u64>,
    ai_id: Option<u64>,
}

fn subject_payload(subject: &Subject) -> SubjectPayload {
    SubjectPayload {
        human_id: subject.human_id.map(|id| id.into_inner()),
        ai_id: subject.ai_id.map(|id| id.into_inner()),
    }
}

#[derive(Serialize)]
struct ToolsListRequest<'a> {
    anchor: AnchorPayload<'a>,
    scene: &'a str,
    capabilities: Vec<String>,
}

#[derive(Deserialize)]
struct ToolsListResponse {
    tools: Vec<ToolDef>,
    policy_digest: Option<String>,
}

#[derive(Serialize)]
struct PrechargeRequest<'a> {
    anchor: AnchorPayload<'a>,
    subject: SubjectPayload,
    tool_id: &'a str,
    idem_key: &'a str,
}

#[derive(Deserialize)]
struct PrechargeResponse {
    decision: String,
    reservation: Option<Value>,
}

#[derive(Serialize)]
struct ExecuteRequest<'a> {
    anchor: AnchorPayload<'a>,
    subject: SubjectPayload,
    call: ToolCallSpec,
}

#[derive(Deserialize)]
struct ExecuteResponse {
    result: TwExecuteResult,
}

#[derive(Serialize)]
struct ReconcileRequest<'a> {
    anchor: AnchorPayload<'a>,
    idem_key: &'a str,
    outcome: Value,
}

#[derive(Serialize)]
struct EmitEventsRequest {
    events: Vec<TwEvent>,
}

impl ThinWaistClient for SoulbaseToolsClient {
    fn tools_list(
        &self,
        anchor: &Anchor,
        scene: &str,
        capabilities: &[String],
    ) -> Result<(Vec<ToolDef>, Option<String>), TwError> {
        let request = ToolsListRequest {
            anchor: anchor_payload(anchor),
            scene,
            capabilities: capabilities.to_vec(),
        };
        let response: ToolsListResponse = self.post_json("v1/tools/list", &request)?;
        Ok((response.tools, response.policy_digest))
    }

    fn precharge(
        &self,
        anchor: &Anchor,
        subject: &Subject,
        tool_id: &str,
        idem_key: &str,
    ) -> Result<PrechargeDecision, TwError> {
        let request = PrechargeRequest {
            anchor: anchor_payload(anchor),
            subject: subject_payload(subject),
            tool_id,
            idem_key,
        };
        let response: PrechargeResponse = self.post_json("v1/tools/precharge", &request)?;
        Ok(PrechargeDecision {
            decision: response.decision,
            reservation: response.reservation,
        })
    }

    fn execute(
        &self,
        anchor: &Anchor,
        subject: &Subject,
        call: &ToolCallSpec,
    ) -> Result<TwExecuteResult, TwError> {
        let request = ExecuteRequest {
            anchor: anchor_payload(anchor),
            subject: subject_payload(subject),
            call: call.clone(),
        };
        let response: ExecuteResponse = self.post_json("v1/tools/execute", &request)?;
        Ok(response.result)
    }

    fn reconcile(&self, anchor: &Anchor, idem_key: &str, outcome: &Value) -> Result<(), TwError> {
        let request = ReconcileRequest {
            anchor: anchor_payload(anchor),
            idem_key,
            outcome: outcome.clone(),
        };
        let _: Value = self.post_json("v1/tools/reconcile", &request)?;
        Ok(())
    }

    fn emit_events(&self, events: &[TwEvent]) -> Result<(), TwError> {
        if events.is_empty() {
            return Ok(());
        }
        let request = EmitEventsRequest {
            events: events.to_vec(),
        };
        let _: Value = self.post_json("v1/tools/emit-events", &request)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct TwClientMock {
    inner: Arc<Mutex<TwMockState>>,
}

#[derive(Default)]
struct TwMockState {
    defs: Vec<ToolDef>,
    policy_digest: Option<String>,
    precharge_queue: VecDeque<Result<PrechargeDecision, TwError>>,
    execute_queue: VecDeque<Result<TwExecuteResult, TwError>>,
    reconcile_calls: Vec<(String, Value)>,
    events: Vec<TwEvent>,
}

impl TwClientMock {
    pub fn with_defs(defs: Vec<ToolDef>) -> Self {
        let mut state = TwMockState::default();
        state.defs = defs;
        Self {
            inner: Arc::new(Mutex::new(state)),
        }
    }

    pub fn push_precharge(&self, decision: Result<PrechargeDecision, TwError>) {
        self.inner
            .lock()
            .unwrap()
            .precharge_queue
            .push_back(decision);
    }

    pub fn push_execute(&self, result: Result<TwExecuteResult, TwError>) {
        self.inner.lock().unwrap().execute_queue.push_back(result);
    }

    pub fn take_reconcile_calls(&self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.inner.lock().unwrap().reconcile_calls)
    }

    pub fn take_events(&self) -> Vec<TwEvent> {
        std::mem::take(&mut self.inner.lock().unwrap().events)
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

    fn precharge(
        &self,
        _anchor: &Anchor,
        _subject: &Subject,
        tool_id: &str,
        idem_key: &str,
    ) -> Result<PrechargeDecision, TwError> {
        let mut state = self.inner.lock().unwrap();
        if let Some(next) = state.precharge_queue.pop_front() {
            state.events.push(TwEvent::ToolCalled {
                tool_id: tool_id.into(),
                idem_key: idem_key.into(),
            });
            return next;
        }
        state.events.push(TwEvent::ToolCalled {
            tool_id: tool_id.into(),
            idem_key: idem_key.into(),
        });
        Ok(PrechargeDecision {
            decision: "Allow".into(),
            reservation: None,
        })
    }

    fn execute(
        &self,
        _anchor: &Anchor,
        _subject: &Subject,
        call: &ToolCallSpec,
    ) -> Result<TwExecuteResult, TwError> {
        let mut state = self.inner.lock().unwrap();
        if let Some(next) = state.execute_queue.pop_front() {
            match &next {
                Ok(_) => state.events.push(TwEvent::ToolResponded {
                    tool_id: call.tool_id.clone(),
                    idem_key: call.idem_key.clone(),
                }),
                Err(err) => state.events.push(TwEvent::ToolFailed {
                    tool_id: call.tool_id.clone(),
                    idem_key: call.idem_key.clone(),
                    reason: err.to_string(),
                }),
            }
            return next;
        }
        state.events.push(TwEvent::ToolResponded {
            tool_id: call.tool_id.clone(),
            idem_key: call.idem_key.clone(),
        });
        Ok(TwExecuteResult {
            summary: ToolResultSummary {
                tool_id: call.tool_id.clone(),
                schema_v: call.schema_v,
                summary: serde_json::json!({"noop": true}),
                evidence_pointer: None,
                result_digest: "noop".into(),
            },
            degradation_reason: None,
            indices_used: None,
            query_hash: None,
        })
    }

    fn reconcile(&self, _anchor: &Anchor, idem_key: &str, outcome: &Value) -> Result<(), TwError> {
        self.inner
            .lock()
            .unwrap()
            .reconcile_calls
            .push((idem_key.into(), outcome.clone()));
        Ok(())
    }

    fn emit_events(&self, events: &[TwEvent]) -> Result<(), TwError> {
        self.inner.lock().unwrap().events.extend_from_slice(events);
        Ok(())
    }
}

impl From<TwError> for ToolError {
    fn from(value: TwError) -> Self {
        ToolError::ThinWaist(value.to_string())
    }
}

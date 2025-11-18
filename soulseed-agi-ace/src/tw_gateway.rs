use std::time::Duration;

use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value, json};
use soulseed_agi_core_models::awareness::AwarenessDegradationReason;
use soulseed_agi_core_models::common::EvidencePointer;
use soulseed_agi_core_models::dialogue_event::payload::{CollaborationPayload, ToolPayload};
use soulseed_agi_core_models::legacy::dialogue_event::{ToolInvocation, ToolResult};
use soulseed_agi_core_models::{SubjectRef, TenantId};
use tracing::warn;

use crate::errors::AceError;
use crate::types::CycleSchedule;

const HEADER_TENANT: &str = "x-tenant-id";

#[derive(Clone)]
pub struct SoulbaseGateway {
    base_url: String,
    client: Client,
    token: String,
}

impl SoulbaseGateway {
    pub fn from_env() -> Option<Result<Self, AceError>> {
        let base = std::env::var("SOULBASE_GATEWAY_BASE_URL").ok()?;
        let token = std::env::var("SOULBASE_GATEWAY_TOKEN").ok()?;
        let timeout = std::env::var("SOULBASE_GATEWAY_TIMEOUT_MS")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(15_000);

        let client = Client::builder()
            .timeout(Duration::from_millis(timeout))
            .build()
            .map_err(|err| {
                AceError::ThinWaist(format!("soulbase gateway client init failed: {err}"))
            });

        Some(client.map(|client| Self {
            base_url: base.trim_end_matches('/').to_string(),
            client,
            token,
        }))
    }

    pub fn execute_tool_plan(
        &self,
        schedule: &CycleSchedule,
        plan: &soulseed_agi_core_models::awareness::ToolPlan,
    ) -> Result<ToolSynthesis, AceError> {
        let tenant = schedule.anchor.tenant_id;
        let url = format!(
            "{}/tenants/{}/tools.execute",
            self.base_url,
            tenant.into_inner()
        );
        let body = json!({
            "anchor": schedule.anchor,
            "plan": plan,
            "cycle": cycle_ctx(schedule),
            "router": router_ctx(schedule),
            "budget": schedule.budget,
        });

        // 使用 tokio runtime 执行 async 调用
        let envelope: GatewayEnvelope<ToolExecuteData> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.post(&url, tenant, body, "tools.execute").await
            })
        })?;

        let data = envelope.data.ok_or_else(|| {
            AceError::ThinWaist("soulbase gateway tools.execute missing data".into())
        })?;

        ToolSynthesis::from_gateway(plan, data)
    }

    pub fn execute_collab_plan(
        &self,
        schedule: &CycleSchedule,
        plan: &soulseed_agi_core_models::awareness::CollabPlan,
    ) -> Result<CollabSynthesis, AceError> {
        let tenant = schedule.anchor.tenant_id;
        let url = format!(
            "{}/tenants/{}/collab.execute",
            self.base_url,
            tenant.into_inner()
        );
        let body = json!({
            "anchor": schedule.anchor,
            "plan": plan,
            "cycle": cycle_ctx(schedule),
            "router": router_ctx(schedule),
            "budget": schedule.budget,
        });

        // 使用 tokio runtime 执行 async 调用
        let envelope: GatewayEnvelope<CollabExecuteData> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.post(&url, tenant, body, "collab.execute").await
            })
        })?;

        let data = envelope.data.ok_or_else(|| {
            AceError::ThinWaist("soulbase gateway collab.execute missing data".into())
        })?;

        CollabSynthesis::from_gateway(schedule, plan, data)
    }

    async fn post<T>(
        &self,
        url: &str,
        tenant: TenantId,
        body: Value,
        endpoint: &str,
    ) -> Result<GatewayEnvelope<T>, AceError>
    where
        for<'de> T: Deserialize<'de>,
    {
        let tenant_raw = tenant.into_inner().to_string();
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|err| AceError::ThinWaist(format!("invalid gateway token: {err}")))?,
        );
        headers.insert(
            HeaderName::from_static(HEADER_TENANT),
            HeaderValue::from_str(&tenant_raw)
                .map_err(|err| AceError::ThinWaist(format!("invalid tenant header: {err}")))?,
        );

        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                AceError::ThinWaist(format!("soulbase gateway request failed: {err}"))
            })?;

        let status = response.status();
        let envelope: GatewayEnvelope<T> = response.json().await.map_err(|err| {
            AceError::ThinWaist(format!(
                "soulbase gateway parse response failed ({endpoint}): {err}"
            ))
        })?;

        if !status.is_success() || !envelope.success {
            let msg = envelope
                .error
                .as_ref()
                .map(|err| format!("{}: {}", err.code, err.message.clone().unwrap_or_default()))
                .unwrap_or_else(|| format!("http {}", status));
            return Err(AceError::ThinWaist(format!(
                "soulbase gateway {endpoint} failed: {msg}"
            )));
        }

        Ok(envelope)
    }
}

fn cycle_ctx(schedule: &CycleSchedule) -> Value {
    json!({
        "cycle_id": schedule.cycle_id,
        "lane": schedule.lane,
        "parent_cycle_id": schedule.parent_cycle_id,
        "collab_scope_id": schedule.collab_scope_id,
    })
}

fn router_ctx(schedule: &CycleSchedule) -> Value {
    json!({
        "context_digest": schedule.router_decision.context_digest,
        "decision_router_digest": schedule.router_decision.plan.explain.router_digest,
        "routing_seed": schedule.router_decision.plan.explain.routing_seed,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayAwareness {
    pub event_type: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier_id: Option<String>,
}

#[derive(Clone)]
pub struct ToolSynthesis {
    pub payload: ToolPayload,
    pub metadata: JsonMap<String, Value>,
    pub manifest: JsonMap<String, Value>,
    pub awareness: Vec<GatewayAwareness>,
}

impl ToolSynthesis {
    fn from_gateway(
        plan: &soulseed_agi_core_models::awareness::ToolPlan,
        data: ToolExecuteData,
    ) -> Result<Self, AceError> {
        let default_fragment = data.default_route_fragment();
        let ToolExecuteData {
            invocation,
            result,
            route_id,
            attempt,
            latency_ms,
            evidence,
            output_digest_sha256,
            blob_ref,
            degradation_reason,
            attributes,
            metadata,
            manifest,
            awareness,
        } = data;

        let mut payload = ToolPayload {
            plan: Some(plan.clone()),
            invocation: Some(invocation),
            result: Some(result),
            barrier_id: plan.barrier.mode.clone(),
            route_id,
            attempt,
            latency_ms,
            evidence: evidence.unwrap_or_default(),
            output_digest_sha256,
            blob_ref,
            degradation_reason: map_degradation(degradation_reason.as_deref()),
            attributes: attributes.unwrap_or_else(|| json!({})),
        };

        if payload.route_id.is_none() {
            payload.route_id = Some(format!("tool-route-{}", default_fragment.clone()));
        }

        let mut metadata = metadata.unwrap_or_default();
        if let Some(route_id) = payload.route_id.clone() {
            metadata
                .entry("tool_route_id")
                .or_insert_with(|| Value::String(route_id));
        }
        if let Some(attempt) = payload.attempt {
            metadata
                .entry("tool_attempt")
                .or_insert_with(|| Value::Number(attempt.into()));
        }
        if let Some(latency) = payload.latency_ms {
            metadata
                .entry("tool_latency_ms")
                .or_insert_with(|| Value::Number(latency.into()));
        }
        metadata.entry("tool_invocation_id").or_insert_with(|| {
            Value::String(
                payload
                    .invocation
                    .as_ref()
                    .map(|inv| inv.call_id.clone())
                    .unwrap_or_else(|| default_fragment),
            )
        });

        let manifest = manifest.unwrap_or_default();

        Ok(Self {
            payload,
            metadata,
            manifest,
            awareness: awareness.unwrap_or_default(),
        })
    }
}

#[derive(Clone)]
pub struct CollabSynthesis {
    pub payload: CollaborationPayload,
    pub metadata: JsonMap<String, Value>,
    pub manifest: JsonMap<String, Value>,
    pub awareness: Vec<GatewayAwareness>,
}

impl CollabSynthesis {
    fn from_gateway(
        schedule: &CycleSchedule,
        plan: &soulseed_agi_core_models::awareness::CollabPlan,
        data: CollabExecuteData,
    ) -> Result<Self, AceError> {
        let CollabExecuteData {
            scope_id,
            participants,
            summary_ref,
            assigned_to,
            degradation_reason,
            attributes,
            metadata,
            manifest,
            awareness,
        } = data;

        let scope_id = scope_id
            .or_else(|| schedule.collab_scope_id.clone())
            .unwrap_or_else(|| format!("collab-{}", schedule.cycle_id.as_u64()));

        let payload = CollaborationPayload {
            scope_id,
            participants: participants.unwrap_or_default(),
            barrier_id: plan.barrier.mode.clone(),
            summary_ref,
            assigned_to,
            degradation_reason: map_degradation(degradation_reason.as_deref()),
            attributes: attributes.unwrap_or_else(|| plan.scope.clone()),
        };

        let mut metadata = metadata.unwrap_or_default();
        metadata
            .entry("collab_scope_id")
            .or_insert_with(|| Value::String(payload.scope_id.clone()));
        if let Some(rounds) = plan.rounds {
            metadata
                .entry("collab_rounds_planned")
                .or_insert_with(|| Value::Number(rounds.into()));
        }

        let manifest = manifest.unwrap_or_default();

        Ok(Self {
            payload,
            metadata,
            manifest,
            awareness: awareness.unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
struct GatewayEnvelope<T> {
    success: bool,
    data: Option<T>,
    error: Option<GatewayError>,
    #[allow(dead_code)]
    trace_id: Option<String>,
}

#[derive(Deserialize)]
struct GatewayError {
    code: String,
    #[serde(default)]
    message: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    details: Option<Value>,
}

#[derive(Deserialize)]
struct ToolExecuteData {
    invocation: ToolInvocation,
    result: ToolResult,
    route_id: Option<String>,
    attempt: Option<u32>,
    latency_ms: Option<u32>,
    evidence: Option<Vec<EvidencePointer>>,
    output_digest_sha256: Option<String>,
    blob_ref: Option<String>,
    degradation_reason: Option<String>,
    attributes: Option<Value>,
    metadata: Option<JsonMap<String, Value>>,
    manifest: Option<JsonMap<String, Value>>,
    awareness: Option<Vec<GatewayAwareness>>,
}

impl ToolExecuteData {
    fn default_route_fragment(&self) -> String {
        self.invocation.call_id.clone()
    }
}

#[derive(Deserialize)]
struct CollabExecuteData {
    scope_id: Option<String>,
    participants: Option<Vec<SubjectRef>>,
    summary_ref: Option<soulseed_agi_core_models::ContentReference>,
    assigned_to: Option<SubjectRef>,
    degradation_reason: Option<String>,
    attributes: Option<Value>,
    metadata: Option<JsonMap<String, Value>>,
    manifest: Option<JsonMap<String, Value>>,
    awareness: Option<Vec<GatewayAwareness>>,
}

fn map_degradation(tag: Option<&str>) -> Option<AwarenessDegradationReason> {
    match tag.unwrap_or_default() {
        "budget_tokens" => Some(AwarenessDegradationReason::BudgetTokens),
        "budget_walltime" => Some(AwarenessDegradationReason::BudgetWalltime),
        "budget_external_cost" => Some(AwarenessDegradationReason::BudgetExternalCost),
        "empty_catalog" => Some(AwarenessDegradationReason::EmptyCatalog),
        "privacy_blocked" => Some(AwarenessDegradationReason::PrivacyBlocked),
        "invalid_plan" => Some(AwarenessDegradationReason::InvalidPlan),
        "clarify_exhausted" => Some(AwarenessDegradationReason::ClarifyExhausted),
        "graph_degraded" => Some(AwarenessDegradationReason::GraphDegraded),
        "envctx_degraded" => Some(AwarenessDegradationReason::EnvctxDegraded),
        other if other.is_empty() => None,
        other => {
            warn!("未知的 Soulbase 降级标签：{other}");
            None
        }
    }
}

use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_stream::stream;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{
        HeaderName, Method, StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    },
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures_core::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use soulseed_agi_ace::aggregator::SyncPointAggregator;
use soulseed_agi_ace::budget::BudgetManager;
use soulseed_agi_ace::ca::CaServiceDefault;
use soulseed_agi_ace::checkpointer::Checkpointer;
use soulseed_agi_ace::emitter::Emitter;
use soulseed_agi_ace::engine::AceEngine;
use soulseed_agi_ace::errors::AceError;
use soulseed_agi_ace::hitl::{HitlInjection, HitlPriority, HitlQueueConfig, HitlService};
use soulseed_agi_ace::metrics::NoopMetrics;
use soulseed_agi_ace::outbox::OutboxService;
use soulseed_agi_ace::persistence::AcePersistence;
use soulseed_agi_ace::runtime::{AceService, TriggerComposer, load_surreal_dotenv_settings};
use soulseed_agi_ace::scheduler::{CycleScheduler, SchedulerConfig};
use soulseed_agi_ace::types::{CycleSchedule, OutboxMessage, SyncPointInput};
use soulseed_agi_core_models::{DialogueEvent, TenantId, awareness::AwarenessEvent};
use soulseed_agi_dfr::filter::CandidateFilter;
use soulseed_agi_dfr::hardgate::HardGate;
use soulseed_agi_dfr::scorer::CandidateScorer;
use soulseed_agi_dfr::{RoutePlanner, RouterService};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::time::sleep;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

#[cfg(any(feature = "outbox-redis", feature = "persistence-surreal"))]
use soulbase_storage::surreal::config::SurrealConfig;
#[cfg(feature = "outbox-redis")]
use soulbase_tx::config::TxConfig;
#[cfg(feature = "outbox-redis")]
use soulbase_tx::transport::redis::RedisTransportConfig;
#[cfg(feature = "outbox-redis")]
use soulseed_agi_ace::outbox::redis::{RedisForwarderConfig, RedisOutboxForwarder};
#[cfg(feature = "outbox-redis")]
use std::sync::Arc as StdArc;

#[derive(Clone)]
struct AppState {
    service: Arc<Mutex<AceService<'static>>>,
    outbox: OutboxService,
    cycles: Arc<Mutex<HashMap<u64, CycleSnapshot>>>,
    persistence: Arc<dyn AcePersistence>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct CycleResponse {
    cycle_id: u64,
    status: String,
    manifest_digest: Option<String>,
}

#[derive(Serialize)]
struct ApiEnvelope<T> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ApiErrorBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

#[derive(Serialize)]
struct ApiErrorBody {
    code: String,
    message: String,
}

impl<T> ApiEnvelope<T> {
    fn success(data: T, duration: Duration) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            trace_id: None,
            duration_ms: Some(duration.as_millis() as u64),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct CycleOutcomeSummary {
    cycle_id: u64,
    status: String,
    manifest_digest: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CycleSnapshot {
    schedule: CycleSchedule,
    sync_point: SyncPointInput,
    outcomes: Vec<CycleOutcomeSummary>,
    outbox: Vec<OutboxMessage>,
}

#[derive(Deserialize)]
struct InjectionRequest {
    cycle_id: u64,
    priority: String,
    author_role: String,
    payload: Value,
}

#[derive(Deserialize, Default)]
struct AwarenessEventsQuery {
    limit: Option<usize>,
}

impl AwarenessEventsQuery {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(200).clamp(1, 500)
    }
}

#[derive(Deserialize, Default)]
struct TimelineQuery {
    limit: Option<usize>,
    session_id: Option<String>,
    cursor: Option<String>,
}

impl TimelineQuery {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(50).clamp(1, 200)
    }
}

#[derive(Serialize, Default)]
struct TimelinePayload {
    #[serde(default)]
    items: Vec<DialogueEvent>,
    #[serde(default)]
    awareness: Vec<AwarenessEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Serialize, Default)]
struct ContextBundleView {
    anchor: ContextAnchor,
    #[serde(default)]
    segments: Vec<Value>,
    #[serde(default)]
    explain: ExplainBundle,
    #[serde(skip_serializing_if = "Option::is_none")]
    budget: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_digest: Option<String>,
}

#[derive(Serialize, Default)]
struct ContextAnchor {
    tenant_id: i64,
    envelope_id: String,
    config_snapshot_hash: String,
    config_snapshot_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<i64>,
    schema_v: u16,
}

#[derive(Serialize, Default)]
struct ExplainBundle {
    #[serde(default)]
    reasons: Vec<String>,
    #[serde(default)]
    indices_used: Vec<String>,
}

#[derive(Serialize, Default)]
struct ExplainIndices {
    #[serde(default)]
    graph: ExplainSection,
    #[serde(default)]
    context: ExplainSection,
    #[serde(default)]
    dfr: DfrExplainSection,
    #[serde(default)]
    ace: AceExplainSection,
}

#[derive(Serialize, Default)]
struct ExplainSection {
    #[serde(default)]
    indices_used: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    degradation_reason: Option<String>,
}

#[derive(Serialize, Default)]
struct DfrExplainSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    router_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    degradation_reason: Option<String>,
}

#[derive(Serialize, Default)]
struct AceExplainSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    degradation_reason: Option<String>,
}

#[derive(Debug)]
enum AppError {
    Ace(AceError),
    Service(String),
    NotFound(String),
}

impl From<AceError> for AppError {
    fn from(err: AceError) -> Self {
        AppError::Ace(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Ace(err) => {
                let status = match err {
                    AceError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
                    AceError::Quota(_) => StatusCode::TOO_MANY_REQUESTS,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, Json(json!({ "error": err.to_string() }))).into_response()
            }
            AppError::Service(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
            AppError::NotFound(message) => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .ok();

    let settings = load_surreal_dotenv_settings().unwrap_or_default();
    let tenant_id = std::env::var("ACE_TENANT_ID")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);

    let outbox = build_outbox_service(&settings, tenant_id)?;
    let outbox_handle = outbox.clone();
    let cycles = Arc::new(Mutex::new(HashMap::new()));
    let engine = build_engine(&outbox);
    let service = AceService::new(engine)?;

    // 获取 persistence 引用用于从数据库加载快照（异步初始化）
    let persistence = init_persistence_for_api().await?;

    let state = AppState {
        service: Arc::new(Mutex::new(service)),
        outbox: outbox_handle,
        cycles: cycles.clone(),
        persistence,
    };

    let bind_addr = std::env::var("ACE_SERVICE_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|err| AppError::Service(format!("invalid ACE_SERVICE_ADDR: {err}")))?;
    info!("ACE service listening on {addr}");

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            AUTHORIZATION,
            CONTENT_TYPE,
            ACCEPT,
            HeaderName::from_static("x-tenant-id"),
        ]);

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/triggers/dialogue", post(trigger_dialogue))
        .route("/api/v1/ace/injections", post(post_injection))
        .route("/api/v1/ace/cycles/:cycle_id", get(get_cycle))
        .route("/api/v1/ace/cycles/:cycle_id/outbox", get(get_cycle_outbox))
        .route("/api/v1/ace/cycles/:cycle_id/stream", get(stream_cycle))
        .route(
            "/api/v1/tenants/:tenant_id/awareness/events",
            get(list_awareness_events),
        )
        .route(
            "/api/v1/tenants/:tenant_id/graph/timeline",
            get(get_timeline),
        )
        .route(
            "/api/v1/tenants/:tenant_id/context/bundle",
            get(get_context_bundle),
        )
        .route(
            "/api/v1/tenants/:tenant_id/explain/indices",
            get(get_explain_indices),
        )
        .layer(cors)
        .with_state(state);

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|err| AppError::Service(format!("failed to bind: {err}")))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| AppError::Service(format!("server error: {err}")))?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

async fn trigger_dialogue(
    State(app): State<AppState>,
    Json(event): Json<DialogueEvent>,
) -> Result<Json<CycleResponse>, AppError> {
    let request = TriggerComposer::cycle_request_from_message(&event);
    let mut service = app
        .service
        .lock()
        .map_err(|_| AppError::Service("service lock poisoned".into()))?;

    let (schedule, sync_point) = service.submit_trigger(request)?;
    let tenant = schedule.anchor.tenant_id.into_inner();
    service.submit_callback(sync_point.clone());  // 将 SyncPoint 入队
    let outcomes = service.drive_until_idle(tenant)?;
    drop(service);

    let outcome_summaries: Vec<CycleOutcomeSummary> = outcomes
        .iter()
        .map(|o| CycleOutcomeSummary {
            cycle_id: o.cycle_id.as_u64(),
            status: format!("{:?}", o.status).to_lowercase(),
            manifest_digest: o.manifest_digest.clone(),
        })
        .collect();

    let latest_status = outcome_summaries
        .last()
        .map(|s| s.status.clone())
        .unwrap_or_else(|| "pending".into());
    let latest_manifest = outcome_summaries
        .last()
        .and_then(|s| s.manifest_digest.clone());

    let outbox_messages = app.outbox.peek(schedule.anchor.tenant_id);

    let snapshot = CycleSnapshot {
        schedule: schedule.clone(),
        sync_point: sync_point.clone(),
        outcomes: outcome_summaries.clone(),
        outbox: outbox_messages.clone(),
    };

    // 保存到内存缓存
    app.cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
        .insert(schedule.cycle_id.as_u64(), snapshot.clone());

    // 持久化到数据库
    let snapshot_value = serde_json::to_value(&snapshot)
        .map_err(|e| AppError::Service(format!("serialize snapshot failed: {e}")))?;
    if let Err(e) = app.persistence.persist_cycle_snapshot(
        schedule.anchor.tenant_id,
        schedule.cycle_id,
        &snapshot_value,
    ) {
        // 持久化失败只记录警告，不影响请求
        tracing::warn!("persist cycle snapshot failed: {}", e);
    }

    Ok(Json(CycleResponse {
        cycle_id: schedule.cycle_id.as_u64(),
        status: latest_status,
        manifest_digest: latest_manifest,
    }))
}

async fn list_awareness_events(
    State(app): State<AppState>,
    Path(tenant_id): Path<u64>,
    Query(query): Query<AwarenessEventsQuery>,
) -> Result<Json<ApiEnvelope<Vec<AwarenessEvent>>>, AppError> {
    let tenant = TenantId::from_raw(tenant_id)
        .map_err(|err| AppError::Service(format!("invalid tenant_id: {err}")))?;
    let limit = query.limit();
    let start = Instant::now();
    let events = app.persistence.list_awareness_events(tenant, limit)?;
    Ok(Json(ApiEnvelope::success(events, start.elapsed())))
}

async fn get_timeline(
    State(app): State<AppState>,
    Path(tenant_id): Path<u64>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<ApiEnvelope<TimelinePayload>>, AppError> {
    let tenant = TenantId::from_raw(tenant_id)
        .map_err(|err| AppError::Service(format!("invalid tenant_id: {err}")))?;
    let limit = query.limit();
    let start = Instant::now();

    // 获取awareness events
    let awareness = app.persistence.list_awareness_events(tenant, limit)?;

    // TODO: 当实现DialogueEvent存储后,从数据库获取items
    // 目前返回空的items列表
    let items = Vec::new();

    let payload = TimelinePayload {
        items,
        awareness,
        next_cursor: None, // TODO: 实现分页游标
    };

    Ok(Json(ApiEnvelope::success(payload, start.elapsed())))
}

async fn get_context_bundle(
    State(_app): State<AppState>,
    Path(tenant_id): Path<u64>,
) -> Result<Json<ApiEnvelope<ContextBundleView>>, AppError> {
    let start = Instant::now();

    // TODO: 实现实际的上下文bundle查询逻辑
    // 目前返回stub数据
    let bundle = ContextBundleView {
        anchor: ContextAnchor {
            tenant_id: tenant_id as i64,
            envelope_id: format!("envelope_{}", tenant_id),
            config_snapshot_hash: "stub_hash".to_string(),
            config_snapshot_version: 1,
            session_id: None,
            schema_v: 1,
        },
        segments: Vec::new(),
        explain: ExplainBundle {
            reasons: vec!["Stub implementation: context bundle endpoint".to_string()],
            indices_used: Vec::new(),
        },
        budget: None,
        manifest_digest: None,
    };

    Ok(Json(ApiEnvelope::success(bundle, start.elapsed())))
}

async fn get_explain_indices(
    State(_app): State<AppState>,
    Path(_tenant_id): Path<u64>,
) -> Result<Json<ApiEnvelope<ExplainIndices>>, AppError> {
    let start = Instant::now();

    // TODO: 实现实际的系统诊断数据查询
    // 目前返回stub数据
    let indices = ExplainIndices {
        graph: ExplainSection {
            indices_used: vec!["graph_timeline_idx".to_string(), "graph_causal_idx".to_string()],
            query_hash: Some("stub_graph_hash".to_string()),
            degradation_reason: None,
        },
        context: ExplainSection {
            indices_used: vec!["context_bundle_idx".to_string()],
            query_hash: Some("stub_context_hash".to_string()),
            degradation_reason: None,
        },
        dfr: DfrExplainSection {
            router_digest: Some("stub_dfr_digest".to_string()),
            degradation_reason: None,
        },
        ace: AceExplainSection {
            degradation_reason: None,
        },
    };

    Ok(Json(ApiEnvelope::success(indices, start.elapsed())))
}

async fn post_injection(
    State(app): State<AppState>,
    Json(req): Json<InjectionRequest>,
) -> Result<Json<CycleSnapshot>, AppError> {
    // 首先从内存缓存查找
    let existing = {
        let guard = app
            .cycles
            .lock()
            .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?;

        if let Some(snapshot) = guard.get(&req.cycle_id).cloned() {
            drop(guard);  // ✅ 立即释放锁！
            snapshot
        } else {
            drop(guard);  // ✅ 释放锁

            // 内存中没有，尝试从数据库加载
            use soulseed_agi_core_models::{AwarenessCycleId, TenantId};

            let tenant_id = TenantId::from_raw_unchecked(1);
            let cycle_id_typed = AwarenessCycleId::from_raw(req.cycle_id)
                .map_err(|e| AppError::Service(format!("invalid cycle_id: {e}")))?;

            match app.persistence.load_cycle_snapshot(tenant_id, cycle_id_typed)? {
                Some(snapshot_value) => {
                    // 尝试反序列化，如果失败则记录警告并返回NotFound
                    let snapshot: CycleSnapshot = match serde_json::from_value(snapshot_value) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to deserialize snapshot for cycle {}: {}. This may be due to schema changes. Treating as not found.",
                                req.cycle_id, e
                            );
                            return Err(AppError::NotFound(format!(
                                "cycle {} snapshot is corrupted or incompatible", req.cycle_id
                            )));
                        }
                    };

                    // 将加载的快照放入缓存
                    app.cycles
                        .lock()
                        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
                        .insert(req.cycle_id, snapshot.clone());

                    snapshot
                }
                None => {
                    return Err(AppError::NotFound(format!("cycle {} not found", req.cycle_id)));
                }
            }
        }
    };

    let priority = parse_priority(&req.priority)?;
    let tenant_id = existing.schedule.anchor.tenant_id;
    let injection = HitlInjection::new(tenant_id, priority, req.author_role.clone(), req.payload);

    let mut sync_point = existing.sync_point.clone();
    sync_point.pending_injections.push(injection);

    let tenant_raw = tenant_id.into_inner();

    // 异步侧信道模式：提交注入但不等待执行完成
    {
        let service = app
            .service
            .lock()
            .map_err(|_| AppError::Service("service lock poisoned".into()))?;

        tracing::info!(
            "post_injection: cycle_id={} (u64={}), sync_point.cycle_id={} (u64={}), tenant={}",
            existing.schedule.cycle_id,
            existing.schedule.cycle_id.as_u64(),
            sync_point.cycle_id,
            sync_point.cycle_id.as_u64(),
            tenant_raw
        );

        // 将周期重新加入调度器队列（服务重启后队列为空）
        service.reschedule_cycle(existing.schedule.clone());

        // 提交包含注入的 SyncPoint（异步侧信道）
        service.submit_callback(sync_point.clone());

        tracing::info!(
            "post_injection: HITL injection submitted to callback queue (async side-channel mode)"
        );
    }

    // 启动后台任务驱动 AC 执行（不阻塞 API 响应）
    let service_for_bg = app.service.clone();
    let cycles_for_bg = app.cycles.clone();
    let cycle_id_for_bg = req.cycle_id;
    tokio::spawn(async move {
        tracing::info!("post_injection: background task started for tenant={}", tenant_raw);

        // 短暂延迟，确保 API 响应先返回
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 驱动执行
        let result = {
            let mut service = match service_for_bg.lock() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("background task: service lock poisoned: {:?}", e);
                    return;
                }
            };
            service.drive_until_idle(tenant_raw)
        };

        match result {
            Ok(outcomes) => {
                tracing::info!("background task: drive_until_idle completed with {} outcomes", outcomes.len());

                // 更新缓存中的快照（可选）
                if let Some(last_outcome) = outcomes.last() {
                    if let Ok(mut guard) = cycles_for_bg.lock() {
                        if let Some(snapshot) = guard.get_mut(&cycle_id_for_bg) {
                            snapshot.schedule.status = last_outcome.status;
                            tracing::info!("background task: updated cached snapshot status to {:?}", last_outcome.status);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("background task: drive_until_idle failed: {:?}", e);
            }
        }
    });

    // 获取当前的 outbox 消息
    let outbox_messages = app.outbox.peek(existing.schedule.anchor.tenant_id);

    // 注意：我们不调用 drive_until_idle，让后台 orchestrator 自然处理
    // AC 的完成由 Agent 自己决定，而不是由外部注入触发
    // 因此返回的快照表示"注入已提交"而非"注入已处理完成"

    let updated = CycleSnapshot {
        schedule: existing.schedule.clone(),  // 返回当前状态，不是执行后的状态
        sync_point: sync_point.clone(),       // 包含新提交的注入
        outcomes: existing.outcomes.clone(),   // 保留之前的 outcomes
        outbox: outbox_messages,
    };

    tracing::info!("post_injection: created CycleSnapshot, attempting to lock cycles cache");

    // 保存到内存缓存
    let lock_result = app.cycles.lock();
    match lock_result {
        Ok(mut guard) => {
            tracing::info!("post_injection: cycles lock acquired successfully");
            guard.insert(req.cycle_id, updated.clone());
            drop(guard);
            tracing::info!("post_injection: cycles lock released");
        }
        Err(e) => {
            tracing::error!("post_injection: cycles lock poisoned: {:?}", e);
            return Err(AppError::Service("cycle cache lock poisoned".into()));
        }
    }

    // 持久化到数据库（异步后台任务，不阻塞响应）
    tracing::info!("post_injection: spawning background task for persistence");
    let persistence = app.persistence.clone();
    let tenant_id = existing.schedule.anchor.tenant_id;
    let snapshot_for_persist = updated.clone();
    use soulseed_agi_core_models::AwarenessCycleId;
    let cycle_id_typed = AwarenessCycleId::from_raw(req.cycle_id)
        .map_err(|e| AppError::Service(format!("invalid cycle_id: {e}")))?;

    tokio::spawn(async move {
        let snapshot_value = match serde_json::to_value(&snapshot_for_persist) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("serialize snapshot failed: {}", e);
                return;
            }
        };

        if let Err(e) = persistence.persist_cycle_snapshot(
            tenant_id,
            cycle_id_typed,
            &snapshot_value,
        ) {
            tracing::warn!("persist cycle snapshot failed: {}", e);
        } else {
            tracing::info!("post_injection: snapshot persisted successfully in background");
        }
    });

    tracing::info!("post_injection: about to return response with status={:?}", updated.schedule.status);

    // 测试：先序列化看看是否卡在这里
    let json_result = serde_json::to_string(&updated);
    match json_result {
        Ok(json_str) => {
            tracing::info!("post_injection: serialization successful, json length={}", json_str.len());
        }
        Err(e) => {
            tracing::error!("post_injection: serialization failed: {}", e);
            return Err(AppError::Service(format!("serialize response failed: {}", e)));
        }
    }

    tracing::info!("post_injection: returning Json response now");
    Ok(Json(updated))
}

async fn get_cycle(
    State(app): State<AppState>,
    Path(cycle_id): Path<u64>,
) -> Result<Json<CycleSnapshot>, AppError> {
    // 首先从内存缓存查找
    let guard = app
        .cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?;
    if let Some(snapshot) = guard.get(&cycle_id).cloned() {
        return Ok(Json(snapshot));
    }
    drop(guard);

    // 内存中没有，尝试从数据库加载
    use soulseed_agi_core_models::{AwarenessCycleId, TenantId};

    // 这里假设使用租户 ID 1，实际应该从请求头或其他地方获取
    let tenant_id = TenantId::from_raw_unchecked(1);
    let cycle_id_typed = AwarenessCycleId::from_raw(cycle_id)
        .map_err(|e| AppError::Service(format!("invalid cycle_id: {e}")))?;

    match app
        .persistence
        .load_cycle_snapshot(tenant_id, cycle_id_typed)?
    {
        Some(snapshot_value) => {
            // 尝试反序列化，如果失败则记录警告并返回NotFound
            let snapshot: CycleSnapshot = match serde_json::from_value(snapshot_value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "Failed to deserialize snapshot for cycle {}: {}. This may be due to schema changes. Treating as not found.",
                        cycle_id, e
                    );
                    return Err(AppError::NotFound(format!(
                        "cycle {cycle_id} snapshot is corrupted or incompatible"
                    )));
                }
            };

            // 将加载的快照放入缓存
            app.cycles
                .lock()
                .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
                .insert(cycle_id, snapshot.clone());

            Ok(Json(snapshot))
        }
        None => Err(AppError::NotFound(format!("cycle {cycle_id} not found"))),
    }
}

async fn get_cycle_outbox(
    State(app): State<AppState>,
    Path(cycle_id): Path<u64>,
) -> Result<Json<Vec<OutboxMessage>>, AppError> {
    // 复用 get_cycle 的逻辑，先查缓存再查数据库
    let guard = app
        .cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?;
    if let Some(snapshot) = guard.get(&cycle_id).cloned() {
        return Ok(Json(snapshot.outbox));
    }
    drop(guard);

    // 内存中没有，尝试从数据库加载
    use soulseed_agi_core_models::{AwarenessCycleId, TenantId};

    let tenant_id = TenantId::from_raw_unchecked(1);
    let cycle_id_typed = AwarenessCycleId::from_raw(cycle_id)
        .map_err(|e| AppError::Service(format!("invalid cycle_id: {e}")))?;

    match app
        .persistence
        .load_cycle_snapshot(tenant_id, cycle_id_typed)?
    {
        Some(snapshot_value) => {
            // 尝试反序列化，如果失败则记录警告并返回NotFound
            let snapshot: CycleSnapshot = match serde_json::from_value(snapshot_value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "Failed to deserialize snapshot for cycle {}: {}. This may be due to schema changes. Treating as not found.",
                        cycle_id, e
                    );
                    return Err(AppError::NotFound(format!(
                        "cycle {cycle_id} snapshot is corrupted or incompatible"
                    )));
                }
            };

            // 将加载的快照放入缓存
            app.cycles
                .lock()
                .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
                .insert(cycle_id, snapshot.clone());

            Ok(Json(snapshot.outbox))
        }
        None => Err(AppError::NotFound(format!("cycle {cycle_id} not found"))),
    }
}

async fn stream_cycle(
    State(app): State<AppState>,
    Path(cycle_id): Path<u64>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let cycles = app.cycles.clone();

    let stream = stream! {
        let mut attempts = 0u32;
        loop {
            let snapshot = cycles
                .lock()
                .ok()
                .and_then(|cache| cache.get(&cycle_id).cloned());
            if let Some(snapshot) = snapshot {
                let data = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".into());
                yield Ok(Event::default().event("complete").data(data));
                break;
            }

            if attempts >= 120 {
                yield Ok(Event::default().event("timeout").data("timeout"));
                break;
            }

            yield Ok(Event::default().event("pending").data("pending"));
            attempts += 1;
            sleep(Duration::from_millis(500)).await;
        }
    };

    Ok(Sse::new(stream))
}

fn parse_priority(input: &str) -> Result<HitlPriority, AppError> {
    match input.to_lowercase().as_str() {
        "p0" | "p0_critical" | "critical" => Ok(HitlPriority::P0Critical),
        "p1" | "p1_high" | "high" => Ok(HitlPriority::P1High),
        "p2" | "p2_medium" | "medium" => Ok(HitlPriority::P2Medium),
        "p3" | "p3_low" | "low" => Ok(HitlPriority::P3Low),
        other => Err(AppError::Ace(AceError::InvalidRequest(format!(
            "unsupported priority: {}",
            other
        )))),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received");
}

fn build_engine(outbox: &OutboxService) -> AceEngine<'static> {
    let scheduler = Box::leak(Box::new(CycleScheduler::new(SchedulerConfig {
        allow_parallel_lanes: true, // 启用并行车道支持，允许多个 Clarify 周期同时运行
        ..SchedulerConfig::default()
    })));
    let budget_mgr = Box::leak(Box::new(BudgetManager::default()));
    let ca_backend = Arc::new(CaServiceDefault::default());
    let aggregator = Box::leak(Box::new(SyncPointAggregator::new(ca_backend)));
    let checkpointer = Box::leak(Box::new(Checkpointer::default()));
    let outbox_ref: &'static OutboxService = Box::leak(Box::new(outbox.clone()));
    let emitter = Box::leak(Box::new(Emitter));
    let hitl = Box::leak(Box::new(HitlService::new(HitlQueueConfig::default())));
    let metrics = Box::leak(Box::new(NoopMetrics::default()));
    let router_service = Box::leak(Box::new(RouterService::new(
        HardGate::default(),
        CandidateFilter::default(),
        CandidateScorer::default(),
        RoutePlanner::default(),
    )));
    let route_planner = Box::leak(Box::new(RoutePlanner::default()));

    AceEngine::new(
        scheduler,
        budget_mgr,
        aggregator,
        checkpointer,
        outbox_ref,
        emitter,
        hitl,
        metrics,
        router_service,
        route_planner,
    )
}

fn build_outbox_service(
    settings: &HashMap<String, String>,
    tenant_id: u64,
) -> Result<OutboxService, AceError> {
    let _ = (settings, tenant_id);
    #[cfg(feature = "outbox-redis")]
    {
        if let Ok(redis_url) = std::env::var("ACE_REDIS_URL") {
            let mut redis_cfg = RedisTransportConfig::default();
            redis_cfg.url = redis_url;
            if let Ok(list_key) = std::env::var("ACE_REDIS_LIST_KEY") {
                if !list_key.is_empty() {
                    redis_cfg.list_key = list_key;
                }
            }
            if let Ok(channel) = std::env::var("ACE_REDIS_CHANNEL") {
                redis_cfg.channel = if channel.is_empty() {
                    None
                } else {
                    Some(channel)
                };
            }

            let surreal_cfg = surreal_config_from_settings(settings);
            let forwarder = RedisOutboxForwarder::new(RedisForwarderConfig {
                surreal: surreal_cfg,
                tx: TxConfig::default(),
                redis: redis_cfg,
                tenant: TenantId::from_raw_unchecked(tenant_id),
            })?;
            return Ok(OutboxService::with_forwarder(StdArc::new(forwarder)));
        }
    }

    Ok(OutboxService::default())
}

#[cfg(any(feature = "outbox-redis", feature = "persistence-surreal"))]
fn surreal_config_from_settings(settings: &HashMap<String, String>) -> SurrealConfig {
    let mut config = SurrealConfig::default();

    if let Some(ns) = settings
        .get("ACE_SURREAL_NAMESPACE")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        config.namespace = ns.into();
    }
    if let Some(db) = settings
        .get("ACE_SURREAL_DATABASE")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        config.database = db.into();
    }
    if let Some(url) = settings
        .get("ACE_SURREAL_URL")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        // Protocol is now embedded in the endpoint string (http://, ws://, etc.)
        config.endpoint = url.into();
    }
    if let Some(pool) = settings
        .get("ACE_SURREAL_POOL_MAX")
        .and_then(|s| s.trim().parse::<usize>().ok())
    {
        config = config.with_pool(pool);
    }
    if let (Ok(username), Ok(password)) = (
        std::env::var("ACE_SURREAL_USERNAME"),
        std::env::var("ACE_SURREAL_PASSWORD"),
    ) {
        config = config.with_credentials(username, password);
    }
    config
}

// 初始化持久化层（用于 API 查询）
#[cfg(feature = "persistence-surreal")]
async fn init_persistence_for_api() -> Result<Arc<dyn AcePersistence>, AppError> {
    use soulseed_agi_ace::persistence::surreal::{SurrealPersistence, SurrealPersistenceConfig};

    let settings = load_surreal_dotenv_settings()
        .map_err(|e| AppError::Service(format!("load settings failed: {e}")))?;

    if settings
        .get("ACE_PERSISTENCE_DISABLED")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
    {
        return Ok(Arc::new(NoopPersistence));
    }

    let config = surreal_config_from_settings(&settings);
    let persistence_config = SurrealPersistenceConfig { datastore: config };

    let persistence = SurrealPersistence::new_async(persistence_config)
        .await
        .map_err(|e| AppError::Service(format!("init persistence failed: {e}")))?;

    Ok(Arc::new(persistence))
}

#[cfg(not(feature = "persistence-surreal"))]
async fn init_persistence_for_api() -> Result<Arc<dyn AcePersistence>, AppError> {
    Ok(Arc::new(NoopPersistence))
}

// Noop persistence implementation
struct NoopPersistence;

impl AcePersistence for NoopPersistence {
    fn persist_cycle(
        &self,
        _emission: &soulseed_agi_ace::types::CycleEmission,
    ) -> Result<(), AceError> {
        Ok(())
    }

    fn persist_cycle_snapshot(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        _snapshot: &Value,
    ) -> Result<(), AceError> {
        Ok(())
    }

    fn load_cycle_snapshot(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _cycle_id: soulseed_agi_core_models::AwarenessCycleId,
    ) -> Result<Option<Value>, AceError> {
        Ok(None)
    }

    fn list_awareness_events(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _limit: usize,
    ) -> Result<Vec<AwarenessEvent>, AceError> {
        Ok(Vec::new())
    }

    fn transactional_checkpoint_and_outbox(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        _snapshot: &Value,
        _outbox_messages: &[soulseed_agi_ace::types::OutboxMessage],
    ) -> Result<(), AceError> {
        Ok(())
    }

    fn list_pending_outbox(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _limit: usize,
    ) -> Result<Vec<soulseed_agi_ace::types::OutboxMessage>, AceError> {
        Ok(Vec::new())
    }

    fn mark_outbox_sent(
        &self,
        _tenant_id: soulseed_agi_core_models::TenantId,
        _event_ids: &[soulseed_agi_core_models::EventId],
    ) -> Result<(), AceError> {
        Ok(())
    }
}

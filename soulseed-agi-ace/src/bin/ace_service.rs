use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_stream::stream;
use futures_core::stream::Stream;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::{Event, Sse}, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use soulseed_agi_ace::aggregator::SyncPointAggregator;
use soulseed_agi_ace::budget::BudgetManager;
use soulseed_agi_ace::ca::CaServiceDefault;
use soulseed_agi_ace::checkpointer::Checkpointer;
use soulseed_agi_ace::emitter::Emitter;
use soulseed_agi_ace::errors::AceError;
use soulseed_agi_ace::hitl::{HitlInjection, HitlPriority, HitlQueueConfig, HitlService};
use soulseed_agi_ace::metrics::NoopMetrics;
use soulseed_agi_ace::outbox::OutboxService;
use soulseed_agi_ace::runtime::{load_surreal_dotenv_settings, AceService, TriggerComposer};
use soulseed_agi_ace::scheduler::{CycleScheduler, SchedulerConfig};
use soulseed_agi_ace::engine::AceEngine;
use soulseed_agi_ace::types::{CycleSchedule, OutboxMessage, SyncPointInput};
use soulseed_agi_core_models::DialogueEvent;
use soulseed_agi_dfr::filter::CandidateFilter;
use soulseed_agi_dfr::hardgate::HardGate;
use soulseed_agi_dfr::scorer::CandidateScorer;
use soulseed_agi_dfr::{RoutePlanner, RouterService};
use tokio::signal;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tracing::info;

#[cfg(feature = "outbox-redis")]
use std::sync::Arc as StdArc;
#[cfg(feature = "outbox-redis")]
use soulseed_agi_ace::outbox::redis::{RedisForwarderConfig, RedisOutboxForwarder};
#[cfg(feature = "outbox-redis")]
use sb_tx::config::TxConfig;
#[cfg(feature = "outbox-redis")]
use sb_tx::transport::redis::RedisTransportConfig;
#[cfg(feature = "outbox-redis")]
use sb_storage::surreal::config::{SurrealConfig, SurrealCredentials, SurrealProtocol};
#[cfg(feature = "outbox-redis")]
use soulseed_agi_core_models::TenantId;

#[derive(Clone)]
struct AppState {
    service: Arc<Mutex<AceService<'static>>>,
    outbox: OutboxService,
    cycles: Arc<Mutex<HashMap<u64, CycleSnapshot>>>,
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

#[derive(Clone, Serialize)]
struct CycleOutcomeSummary {
    cycle_id: u64,
    status: String,
    manifest_digest: Option<String>,
}

#[derive(Clone, Serialize)]
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
            AppError::NotFound(message) => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": message })),
            )
                .into_response(),
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

    let state = AppState {
        service: Arc::new(Mutex::new(service)),
        outbox: outbox_handle,
        cycles: cycles.clone(),
    };

    let bind_addr = std::env::var("ACE_SERVICE_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|err| AppError::Service(format!("invalid ACE_SERVICE_ADDR: {err}")))?;
    info!("ACE service listening on {addr}");

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/triggers/dialogue", post(trigger_dialogue))
        .route("/api/v1/ace/injections", post(post_injection))
        .route("/api/v1/ace/cycles/:cycle_id", get(get_cycle))
        .route(
            "/api/v1/ace/cycles/:cycle_id/outbox",
            get(get_cycle_outbox),
        )
        .route(
            "/api/v1/ace/cycles/:cycle_id/stream",
            get(stream_cycle),
        )
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

    let outbox_messages = app
        .outbox
        .peek(schedule.anchor.tenant_id);

    let snapshot = CycleSnapshot {
        schedule: schedule.clone(),
        sync_point: sync_point.clone(),
        outcomes: outcome_summaries.clone(),
        outbox: outbox_messages.clone(),
    };
    app.cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
        .insert(schedule.cycle_id.as_u64(), snapshot);

    Ok(Json(CycleResponse {
        cycle_id: schedule.cycle_id.as_u64(),
        status: latest_status,
        manifest_digest: latest_manifest,
    }))
}

async fn post_injection(
    State(app): State<AppState>,
    Json(req): Json<InjectionRequest>,
) -> Result<Json<CycleSnapshot>, AppError> {
    let existing = app
        .cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
        .get(&req.cycle_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("cycle {} not found", req.cycle_id)))?;

    let priority = parse_priority(&req.priority)?;
    let tenant_id = existing.schedule.anchor.tenant_id;
    let injection = HitlInjection::new(tenant_id, priority, req.author_role.clone(), req.payload);

    let mut sync_point = existing.sync_point.clone();
    sync_point.pending_injections.push(injection);

    let tenant_raw = tenant_id.into_inner();

    let outcomes = {
        let mut service = app
            .service
            .lock()
            .map_err(|_| AppError::Service("service lock poisoned".into()))?;
        service.submit_callback(sync_point.clone());
        service.drive_until_idle(tenant_raw)?
    };

    let outcome_summaries: Vec<CycleOutcomeSummary> = outcomes
        .iter()
        .map(|o| CycleOutcomeSummary {
            cycle_id: o.cycle_id.as_u64(),
            status: format!("{:?}", o.status).to_lowercase(),
            manifest_digest: o.manifest_digest.clone(),
        })
        .collect();

    let outbox_messages = app.outbox.peek(existing.schedule.anchor.tenant_id);

    let updated = CycleSnapshot {
        schedule: existing.schedule,
        sync_point: sync_point.clone(),
        outcomes: outcome_summaries,
        outbox: outbox_messages,
    };

    app.cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?
        .insert(req.cycle_id, updated.clone());

    Ok(Json(updated))
}

async fn get_cycle(
    State(app): State<AppState>,
    Path(cycle_id): Path<u64>,
) -> Result<Json<CycleSnapshot>, AppError> {
    let guard = app
        .cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?;
    let snapshot = guard
        .get(&cycle_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("cycle {cycle_id} not found")))?;
    Ok(Json(snapshot))
}

async fn get_cycle_outbox(
    State(app): State<AppState>,
    Path(cycle_id): Path<u64>,
) -> Result<Json<Vec<OutboxMessage>>, AppError> {
    let guard = app
        .cycles
        .lock()
        .map_err(|_| AppError::Service("cycle cache lock poisoned".into()))?;
    let snapshot = guard
        .get(&cycle_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("cycle {cycle_id} not found")))?;
    Ok(Json(snapshot.outbox))
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
    let scheduler = Box::leak(Box::new(CycleScheduler::new(SchedulerConfig::default())));
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

#[cfg(feature = "outbox-redis")]
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
        if url.starts_with("http") {
            config.endpoint = url.into();
            config.protocol = SurrealProtocol::Http;
        } else {
            config.endpoint = url.into();
            config.protocol = SurrealProtocol::Ws;
        }
    }
    if let Some(pool) = settings
        .get("ACE_SURREAL_POOL_MAX")
        .and_then(|s| s.trim().parse::<usize>().ok())
    {
        config = config.with_pool(pool);
    }
    if let (Ok(username), Ok(password)) =
        (std::env::var("ACE_SURREAL_USERNAME"), std::env::var("ACE_SURREAL_PASSWORD"))
    {
        config = config.with_credentials(SurrealCredentials::new(username, password));
    }
    config
}

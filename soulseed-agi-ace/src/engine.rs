use crate::aggregator::SyncPointAggregator;
use crate::budget::BudgetManager;
use crate::ca::InjectionAction;
use crate::checkpointer::Checkpointer;
use crate::collab::BarrierManager;
use crate::emitter::Emitter;
use crate::errors::AceError;
use crate::hitl::HitlService;
use crate::metrics::AceMetrics;
use crate::outbox::OutboxService;
use crate::persistence::AcePersistence;
use crate::scheduler::CycleScheduler;
use crate::types::{
    AggregationOutcome, BudgetSnapshot, CycleEmission, CycleLane, CycleRequest, CycleSchedule,
    CycleStatus, ScheduleOutcome, SyncPointInput,
};
use serde_json::{Map, Value, json};
use soulseed_agi_context::types::ContextBundle;
use soulseed_agi_core_models::awareness::{AwarenessEventType, SyncPointKind};
use soulseed_agi_core_models::common::EvidencePointer;
use soulseed_agi_core_models::{AccessClass, AwarenessCycleId, DialogueEvent};
use soulseed_agi_dfr::{DfrEngine, RoutePlanner, RouterService};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use time::{Duration, OffsetDateTime};

pub struct AceEngine<'a> {
    pub scheduler: &'a CycleScheduler,
    pub budget: &'a BudgetManager,
    pub aggregator: &'a SyncPointAggregator,
    pub checkpointer: &'a Checkpointer,
    pub outbox: &'a OutboxService,
    pub emitter: &'a Emitter,
    pub hitl: &'a HitlService,
    pub metrics: &'a dyn AceMetrics,
    pub router: &'a RouterService,
    pub route_planner: &'a RoutePlanner,
    pub barrier_manager: BarrierManager,
    pub lane_cooldown: Duration,
    pub persistence: Option<Arc<dyn AcePersistence>>,
    finalized: Arc<Mutex<HashSet<u64>>>,
}

impl<'a> AceEngine<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scheduler: &'a CycleScheduler,
        budget: &'a BudgetManager,
        aggregator: &'a SyncPointAggregator,
        checkpointer: &'a Checkpointer,
        outbox: &'a OutboxService,
        emitter: &'a Emitter,
        hitl: &'a HitlService,
        metrics: &'a dyn AceMetrics,
        router: &'a RouterService,
        route_planner: &'a RoutePlanner,
    ) -> Self {
        Self {
            scheduler,
            budget,
            aggregator,
            checkpointer,
            outbox,
            emitter,
            hitl,
            metrics,
            router,
            route_planner,
            barrier_manager: BarrierManager::new(),
            lane_cooldown: Duration::seconds(2),
            persistence: None,
            finalized: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn set_persistence(&mut self, persistence: Arc<dyn AcePersistence>) {
        self.persistence = Some(persistence);
    }

    pub fn schedule_cycle(&self, request: CycleRequest) -> Result<ScheduleOutcome, AceError> {
        let decide_start = Instant::now();
        let dfr = DfrEngine::new(self.router, self.route_planner);
        let decision = dfr
            .route(request.router_input.clone(), request.candidates.clone())
            .map_err(|err| AceError::Dfr(err.to_string()))?;
        let lane: CycleLane = decision.plan.fork.into();
        let anchor = decision.plan.anchor.clone();
        let tenant_raw = anchor.tenant_id.into_inner();
        self.checkpointer
            .ensure_lane_idle(tenant_raw, &lane, self.lane_cooldown)?;

        let decide_latency_ms = decide_start.elapsed().as_secs_f64() * 1000.0;
        self.metrics.gauge(
            "ace.decide.latency_ms",
            decide_latency_ms,
            &[("lane", format!("{:?}", lane))],
        );

        let outcome = self.scheduler.schedule(
            decision,
            request.budget.clone(),
            request.parent_cycle_id,
            request.collab_scope_id.clone(),
        )?;

        if let Some(cycle) = &outcome.cycle {
            self.checkpointer.record(crate::types::CheckpointState {
                tenant_id: cycle.anchor.tenant_id,
                cycle_id: cycle.cycle_id,
                lane: cycle.lane.clone(),
                budget: cycle.budget.clone(),
                since: cycle.created_at,
            });
            if matches!(cycle.lane, CycleLane::Clarify) {
                self.hitl.mark_cycle_active(cycle.anchor.tenant_id);
            }
            let labels = vec![
                ("lane", format!("{:?}", cycle.lane)),
                ("fork", format!("{:?}", cycle.router_decision.plan.fork)),
            ];
            self.metrics.counter("ace.schedule.accepted", 1.0, &labels);
        } else {
            let mut labels = vec![("lane", format!("{:?}", lane))];
            if let Some(reason) = outcome.reason.clone() {
                labels.push(("reason", reason));
            }
            self.metrics.counter("ace.schedule.rejected", 1.0, &labels);
        }
        Ok(outcome)
    }

    /// 生成子觉知周期（用于协同场景的分形递归）
    ///
    /// 在Collab路径下，可以为每个子Agent创建独立的觉知周期，
    /// 这些子周期将继承父周期的上下文（按可见域裁剪），
    /// 并通过Barrier机制聚合结果
    pub fn spawn_child_cycle(
        &self,
        parent_schedule: &CycleSchedule,
        collab_plan: &soulseed_agi_core_models::awareness::CollabPlan,
        child_input: &soulseed_agi_dfr::types::RouterInput,
        child_budget: BudgetSnapshot,
    ) -> Result<ScheduleOutcome, AceError> {
        // 1. 验证父周期是Collab路径
        if !matches!(parent_schedule.lane, CycleLane::Collab) {
            return Err(AceError::Dfr(
                "spawn_child_cycle只能在Collab路径下调用".into(),
            ));
        }

        let scope_id = parent_schedule.collab_scope_id.clone()
            .unwrap_or_else(|| format!("collab_{}", parent_schedule.cycle_id.as_u64()));

        // 2. 创建Barrier群组（如果还未创建）
        // 使用 rounds 作为期望的子周期数量提示，如果没有则使用默认值10
        // 实际的子周期数量在 absorb_child_result 时动态确定
        let expected_children = collab_plan.rounds.unwrap_or(10) as usize;
        if self.barrier_manager.get_group(parent_schedule.cycle_id).is_none() {
            self.barrier_manager.create_group(
                parent_schedule.cycle_id,
                scope_id.clone(),
                expected_children,
            )?;

            tracing::info!(
                "Created barrier group for parent_cycle={}, expected_children={}",
                parent_schedule.cycle_id.as_u64(),
                expected_children
            );
        }

        // 3. 应用隐私策略：过滤上下文，只保留Public内容
        let mut filtered_input = child_input.clone();
        filtered_input.context = filter_context_for_collab(&child_input.context);

        tracing::info!(
            "Privacy filter applied for child_cycle: parent={}, original_segments={}, filtered_segments={}",
            parent_schedule.cycle_id.as_u64(),
            child_input.context.segments.len(),
            filtered_input.context.segments.len()
        );

        // 4. 构建子周期的CycleRequest
        let child_request = CycleRequest {
            router_input: filtered_input,
            candidates: vec![], // 子周期使用默认候选路径
            budget: child_budget,
            parent_cycle_id: Some(parent_schedule.cycle_id),
            collab_scope_id: Some(scope_id),
        };

        // 5. 调用schedule_cycle创建子周期
        let outcome = self.schedule_cycle(child_request)?;

        // 6. 注册子周期到Barrier群组
        if let Some(cycle) = &outcome.cycle {
            self.barrier_manager.register_child(
                parent_schedule.cycle_id,
                cycle.cycle_id,
            )?;

            self.metrics.counter(
                "ace.child_cycle.spawned",
                1.0,
                &[
                    ("parent_cycle_id", parent_schedule.cycle_id.as_u64().to_string()),
                    ("child_cycle_id", cycle.cycle_id.as_u64().to_string()),
                    ("child_lane", format!("{:?}", cycle.lane)),
                ],
            );
        }

        Ok(outcome)
    }

    /// 吸收子周期结果（Barrier聚合）
    ///
    /// 当一个子周期完成时，父周期通过此方法吸收子周期的结果。
    /// 这是分形递归的关键步骤，实现了跨层级的状态聚合。
    pub fn absorb_child_result(
        &self,
        parent_cycle_id: AwarenessCycleId,
        child_cycle_id: AwarenessCycleId,
        child_emission: &CycleEmission,
    ) -> Result<(), AceError> {
        // 1. 验证child_cycle_id确实是parent_cycle_id的子周期
        if child_emission.parent_cycle_id != Some(parent_cycle_id) {
            return Err(AceError::Dfr(format!(
                "child_cycle {} 不是 parent_cycle {} 的子周期",
                child_cycle_id.as_u64(),
                parent_cycle_id.as_u64()
            )));
        }

        // 2. 通过BarrierManager记录子周期完成
        let is_barrier_ready = self.barrier_manager.complete_child(
            parent_cycle_id,
            child_cycle_id,
            child_emission.clone(),
        )?;

        // 3. 记录子周期吸收指标
        self.metrics.counter(
            "ace.child_cycle.absorbed",
            1.0,
            &[
                ("parent_cycle_id", parent_cycle_id.as_u64().to_string()),
                ("child_cycle_id", child_cycle_id.as_u64().to_string()),
                ("child_status", format!("{:?}", child_emission.status)),
                ("barrier_ready", if is_barrier_ready { "true" } else { "false" }.to_string()),
            ],
        );

        // 4. 如果Barrier就绪，执行聚合逻辑
        if is_barrier_ready {
            self.perform_barrier_aggregation(parent_cycle_id)?;
        }

        Ok(())
    }

    /// 执行Barrier聚合
    ///
    /// 当所有子周期都完成时，将子周期结果聚合并注入到父周期
    fn perform_barrier_aggregation(
        &self,
        parent_cycle_id: AwarenessCycleId,
    ) -> Result<(), AceError> {
        // 1. 获取Barrier群组
        let group = self.barrier_manager.get_group(parent_cycle_id)
            .ok_or_else(|| AceError::ScheduleConflict(format!(
                "Barrier group not found for parent {}",
                parent_cycle_id.as_u64()
            )))?;

        // 2. 收集所有子周期的结果
        let completed_emissions = group.get_completed_emissions();

        // 3. 构建聚合payload
        let aggregated_results: Vec<Value> = completed_emissions
            .iter()
            .map(|emission| {
                json!({
                    "child_cycle_id": emission.cycle_id.as_u64(),
                    "child_lane": format!("{:?}", emission.lane),
                    "child_status": format!("{:?}", emission.status),
                    "final_event": emission.final_event,
                    "context_manifest": emission.context_manifest,
                })
            })
            .collect();

        // 4. 将聚合结果作为异步回执注入到父周期
        let payload = json!({
            "type": "barrier_aggregation",
            "barrier_id": group.scope_id,
            "total_children": group.expected_children,
            "completed_children": completed_emissions.len(),
            "results": aggregated_results,
        });

        // 使用第一个子周期的tenant_id（所有子周期应该有相同的tenant_id）
        let tenant_id = completed_emissions
            .first()
            .map(|e| e.anchor.tenant_id)
            .ok_or_else(|| AceError::ThinWaist("No completed children found".to_string()))?;

        let injection = crate::hitl::HitlInjection::new(
            tenant_id,
            crate::hitl::HitlPriority::P1High,
            "system:barrier_aggregation",
            payload,
        );

        self.hitl.enqueue(injection);

        // 5. 标记Barrier群组已完成并清理
        self.barrier_manager.finish_group(parent_cycle_id)?;

        // 6. 记录Barrier聚合完成指标
        self.metrics.counter(
            "ace.barrier.aggregated",
            1.0,
            &[
                ("parent_cycle_id", parent_cycle_id.as_u64().to_string()),
                ("children_count", completed_emissions.len().to_string()),
            ],
        );

        tracing::info!(
            "Barrier aggregation completed: parent_cycle={}, children_count={}",
            parent_cycle_id.as_u64(),
            completed_emissions.len()
        );

        Ok(())
    }

    pub fn evaluate_budget(
        &self,
        cycle_id: AwarenessCycleId,
        lane: &CycleLane,
        budget: BudgetSnapshot,
    ) -> Result<crate::types::BudgetDecision, AceError> {
        let decision = self.budget.evaluate(cycle_id, lane, budget.clone())?;
        let tags = vec![
            ("lane", format!("{:?}", lane)),
            (
                "allowed",
                if decision.allowed {
                    "true".into()
                } else {
                    "false".into()
                },
            ),
        ];
        self.metrics.gauge(
            "ace.budget.snapshot.tokens",
            budget.tokens_spent as f64,
            &tags,
        );
        self.metrics.gauge(
            "ace.budget.snapshot.walltime_ms",
            budget.walltime_ms_used as f64,
            &tags,
        );
        self.metrics.gauge(
            "ace.budget.snapshot.external_cost",
            budget.external_cost_spent as f64,
            &tags,
        );
        if let Some(reason) = &decision.degradation_reason {
            let labels = vec![("lane", format!("{:?}", lane)), ("reason", reason.clone())];
            self.metrics.counter("ace.budget.degrade", 1.0, &labels);
        }
        Ok(decision)
    }

    pub fn absorb_sync_point(
        &self,
        mut input: SyncPointInput,
    ) -> Result<AggregationOutcome, AceError> {
        if input.pending_injections.is_empty() {
            let topk = self.hitl.peek_clarify_topk(input.anchor.tenant_id, 3);
            if !topk.is_empty() {
                input.pending_injections = topk.into_iter().map(|entry| entry.injection).collect();
            }
        }
        let (pre_status, _) = status_transition_for_syncpoint(input.kind);
        if let Some(status) = pre_status {
            self.scheduler.mark_status(input.cycle_id, status);
        }
        let outcome = self.aggregator.aggregate(input)?;
        let (_, post_status) = status_transition_for_syncpoint(outcome.report.kind);
        if let Some(status) = post_status {
            self.scheduler.mark_status(outcome.report.cycle_id, status);
        } else if matches!(
            self.scheduler.status_of(outcome.report.cycle_id),
            Some(CycleStatus::AwaitingExternal | CycleStatus::Suspended)
        ) {
            self.scheduler
                .mark_status(outcome.report.cycle_id, CycleStatus::Running);
        }
        for decision in &outcome.injections {
            if matches!(
                decision.action,
                InjectionAction::Applied | InjectionAction::Ignored
            ) {
                self.hitl.resolve_injection(decision.injection_id);
            }
        }
        let labels = vec![
            ("kind", format!("{:?}", outcome.report.kind)),
            ("cycle_id", outcome.report.cycle_id.as_u64().to_string()),
        ];
        self.metrics.counter("ace.syncpoint.absorbed", 1.0, &labels);
        Ok(outcome)
    }

    pub fn finalize_cycle(&self, mut emission: CycleEmission) -> Result<(), AceError> {
        let tenant_id = emission.anchor.tenant_id.into_inner();

        if emission.final_event.base.ac_id.is_none() {
            emission.final_event.base.ac_id = Some(emission.cycle_id);
        }
        if emission.final_event.base.parent_ac_id.is_none() {
            emission.final_event.base.parent_ac_id = emission.parent_cycle_id;
        }
        emission.final_event = sanitize_final_event(&emission.lane, &emission.final_event);
        if emission.final_event.base.ac_id.is_none() {
            emission.final_event.base.ac_id = Some(emission.cycle_id);
        }
        if emission.final_event.base.parent_ac_id.is_none() {
            emission.final_event.base.parent_ac_id = emission.parent_cycle_id;
        }

        let cycle_key = emission.cycle_id.as_u64();
        let mut registry = self.finalized.lock().unwrap();
        if registry.contains(&cycle_key) {
            drop(registry);
            let late_event = soulseed_agi_core_models::awareness::AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: emission.final_event.base.event_id,
                event_type: AwarenessEventType::LateReceiptObserved,
                occurred_at_ms: OffsetDateTime::now_utc().unix_timestamp() * 1000,
                awareness_cycle_id: emission.cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: json!({
                    "late_event_id": emission.final_event.base.event_id.as_u64(),
                    "original_timestamp": emission.final_event.base.timestamp_ms,
                }),
            };
            let envelope = crate::types::OutboxEnvelope {
                tenant_id: emission.anchor.tenant_id,
                cycle_id: emission.cycle_id,
                messages: vec![crate::types::OutboxMessage {
                    cycle_id: emission.cycle_id,
                    event_id: late_event.event_id,
                    payload: late_event,
                }],
            };
            self.outbox.enqueue(envelope)?;
            self.metrics.counter("ace.cycle.late_receipt", 1.0, &[]);
            return Ok(());
        }

        registry.insert(cycle_key);
        drop(registry);

        // 持久化改为异步后台任务，避免在 axum handler 中阻塞
        if let Some(store) = self.persistence.as_ref() {
            let store_clone = store.clone();
            let emission_clone = emission.clone();
            std::thread::spawn(move || {
                if let Err(e) = store_clone.persist_cycle(&emission_clone) {
                    tracing::warn!("persist_cycle failed in background: {}", e);
                } else {
                    tracing::debug!("persist_cycle succeeded in background for cycle_id={}", emission_clone.cycle_id);
                }
            });
        }

        let result = (|| -> Result<(), AceError> {
            let envelope = self.emitter.emit(emission.clone())?;
            let mut reservation = self.outbox.reserve(envelope);
            reservation.commit()?;
            self.checkpointer.finish(tenant_id);
            self.scheduler
                .finish(tenant_id, emission.cycle_id, emission.status);
            if matches!(emission.lane, CycleLane::Clarify) {
                self.hitl.clear_cycle_active(emission.anchor.tenant_id);
            }
            let labels = vec![("lane", format!("{:?}", emission.lane))];
            self.metrics.counter("ace.cycle.finalized", 1.0, &labels);
            Ok(())
        })();

        if let Err(err) = result {
            let mut registry = self.finalized.lock().unwrap();
            registry.remove(&cycle_key);
            return Err(err);
        }
        Ok(())
    }
}

fn sanitize_final_event(
    lane: &CycleLane,
    event: &soulseed_agi_core_models::DialogueEvent,
) -> soulseed_agi_core_models::DialogueEvent {
    let mut legacy: soulseed_agi_core_models::legacy::dialogue_event::DialogueEvent =
        event.clone().into();
    if matches!(lane, CycleLane::Collab | CycleLane::SelfReason) {
        legacy.reasoning_trace = None;
        legacy.reasoning_confidence = None;
        legacy.reasoning_strategy = None;
        legacy.metadata = merge_metadata(&legacy.metadata, lane);
        if legacy.evidence_pointer.is_none() {
            if let Some(digest) = legacy.content_digest_sha256.clone() {
                legacy.evidence_pointer = Some(EvidencePointer {
                    uri: format!("context://summary/{}", legacy.event_id.as_u64()),
                    digest_sha256: Some(digest.clone()),
                    media_type: Some("text/plain".into()),
                    blob_ref: None,
                    span: None,
                    access_policy: Some("summary_only".into()),
                });
            }
        }
    }
    let sanitized = soulseed_agi_core_models::convert_legacy_dialogue_event(legacy);
    validate_event(&sanitized);
    sanitized
}

fn merge_metadata(original: &Value, lane: &CycleLane) -> Value {
    let mut map = match original {
        Value::Object(obj) => obj.clone(),
        _ => Map::new(),
    };
    map.insert("summary_only".into(), Value::Bool(true));
    map.insert("lane".into(), Value::String(format!("{:?}", lane)));
    Value::Object(map)
}

#[cfg_attr(not(test), allow(dead_code))]
fn validate_event(event: &soulseed_agi_core_models::DialogueEvent) {
    if let Err(err) = soulseed_agi_core_models::validate_dialogue_event(event) {
        debug_assert!(false, "dialogue event validation failed: {:?}", err);
    }
}

fn status_transition_for_syncpoint(
    kind: SyncPointKind,
) -> (Option<CycleStatus>, Option<CycleStatus>) {
    use CycleStatus::*;
    match kind {
        SyncPointKind::ClarifyWindowOpened | SyncPointKind::HitlWindowOpened => {
            (Some(AwaitingExternal), None)
        }
        SyncPointKind::ToolWindowOpened
        | SyncPointKind::ToolBarrier
        | SyncPointKind::ToolBarrierReached
        | SyncPointKind::ToolBarrierTimeout
        | SyncPointKind::BudgetExceeded => (Some(Suspended), None),
        SyncPointKind::ClarifyAnswered
        | SyncPointKind::ClarifyWindowClosed
        | SyncPointKind::ToolWindowClosed
        | SyncPointKind::ToolBarrierReleased
        | SyncPointKind::CollabTurnEnd
        | SyncPointKind::HitlAbsorb
        | SyncPointKind::HitlWindowClosed
        | SyncPointKind::Merged
        | SyncPointKind::LateSignalObserved
        | SyncPointKind::DriftDetected
        | SyncPointKind::BudgetRecovered
        | SyncPointKind::ToolChainNext
        | SyncPointKind::DegradationRecorded => (None, Some(Running)),
        _ => (None, None),
    }
}

pub trait CycleRuntime {
    fn prepare_sync_point(&mut self, schedule: &CycleSchedule) -> Result<SyncPointInput, AceError>;

    fn produce_final_event(
        &mut self,
        schedule: &CycleSchedule,
        aggregation: &AggregationOutcome,
    ) -> Result<(DialogueEvent, CycleStatus), AceError>;
}

#[derive(Clone, Debug)]
pub struct CycleOutcome {
    pub cycle_id: AwarenessCycleId,
    pub status: CycleStatus,
    pub manifest_digest: Option<String>,
}

pub struct AceOrchestrator<'a, R> {
    engine: &'a AceEngine<'a>,
    runtime: R,
}

impl<'a, R> AceOrchestrator<'a, R>
where
    R: CycleRuntime,
{
    pub fn new(engine: &'a AceEngine<'a>, runtime: R) -> Self {
        Self { engine, runtime }
    }

    pub fn drive_once(&mut self, tenant: u64) -> Result<Option<CycleOutcome>, AceError> {
        let Some(schedule) = self.engine.scheduler.start_next(tenant) else {
            return Ok(None);
        };

        let mut syncpoint = self.runtime.prepare_sync_point(&schedule)?;
        if syncpoint.parent_cycle_id.is_none() {
            syncpoint.parent_cycle_id = schedule.parent_cycle_id;
        }
        if syncpoint.collab_scope_id.is_none() {
            syncpoint.collab_scope_id = schedule.collab_scope_id.clone();
        }

        let aggregation = self.engine.absorb_sync_point(syncpoint)?;
        let (mut final_event, status) =
            self.runtime.produce_final_event(&schedule, &aggregation)?;

        if final_event.base.ac_id.is_none() {
            final_event.base.ac_id = Some(schedule.cycle_id);
        }
        if final_event.base.parent_ac_id.is_none() {
            final_event.base.parent_ac_id = schedule.parent_cycle_id;
        }

        let mut awareness_events = schedule.decision_events.clone();
        awareness_events.extend(aggregation.awareness_events.clone());

        // 添加 Finalized 事件到 awareness_events，以便持久化到数据库
        // 这样前端可以通过查询觉知事件来判断周期是否完成
        let finalized_event = soulseed_agi_core_models::awareness::AwarenessEvent {
            anchor: schedule.anchor.clone(),
            event_id: final_event.base.event_id,
            event_type: soulseed_agi_core_models::awareness::AwarenessEventType::Finalized,
            occurred_at_ms: final_event.base.timestamp_ms,
            awareness_cycle_id: schedule.cycle_id,
            parent_cycle_id: schedule.parent_cycle_id,
            collab_scope_id: schedule.collab_scope_id.clone(),
            barrier_id: None,
            env_mode: None,
            inference_cycle_sequence: 1,
            degradation_reason: None,
            payload: serde_json::json!({
                "lane": format!("{:?}", schedule.lane),
                "final_event_id": final_event.base.event_id.as_u64(),
            }),
        };
        awareness_events.push(finalized_event);

        let explain_fingerprint = aggregation
            .explain_fingerprint
            .clone()
            .or(schedule.explain_fingerprint.clone());

        let manifest_digest = aggregation
            .context_manifest
            .as_ref()
            .and_then(|val| val.get("manifest_digest"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let emission = CycleEmission {
            cycle_id: schedule.cycle_id,
            lane: schedule.lane.clone(),
            final_event,
            awareness_events,
            budget: aggregation.report.budget_snapshot.clone(),
            anchor: schedule.anchor.clone(),
            explain_fingerprint,
            router_decision: schedule.router_decision.clone(),
            status,
            parent_cycle_id: schedule.parent_cycle_id,
            collab_scope_id: schedule.collab_scope_id.clone(),
            manifest_digest: manifest_digest.clone(),
            context_manifest: aggregation.context_manifest.clone(),
        };

        self.engine.finalize_cycle(emission)?;

        Ok(Some(CycleOutcome {
            cycle_id: schedule.cycle_id,
            status,
            manifest_digest,
        }))
    }

    pub fn drive_until_idle(&mut self, tenant: u64) -> Result<Vec<CycleOutcome>, AceError> {
        let mut outcomes = Vec::new();
        while let Some(outcome) = self.drive_once(tenant)? {
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }
}

/// 为Collab场景过滤上下文，只保留Public访问级别的内容
///
/// 在协同场景下，子Agent不应该看到父周期的私有上下文。
/// 此函数会：
/// 1. 将anchor的access_class设置为Public
/// 2. 过滤掉所有非Public的BundleItem（通过检查evidence_ptrs的access_policy）
///
/// # Arguments
/// * `context` - 原始的ContextBundle
///
/// # Returns
/// 过滤后只包含公开内容的ContextBundle
fn filter_context_for_collab(context: &ContextBundle) -> ContextBundle {
    let mut filtered = context.clone();

    // 1. 将anchor的access_class设置为Public，标记子周期只能访问公开内容
    filtered.anchor.access_class = AccessClass::Public;

    // 2. 过滤segments中的items，只保留公开内容
    for segment in &mut filtered.segments {
        segment.items.retain(|item| {
            // 检查item的evidence_ptrs中的access_policy
            // 如果所有evidence_ptr都是public，或者没有evidence_ptr，则保留
            if item.evidence_ptrs.is_empty() {
                // 没有evidence_ptr，默认为公开
                return true;
            }

            // 只保留所有evidence_ptr的access_policy都是"public"的item
            item.evidence_ptrs.iter().all(|ptr| {
                ptr.access_policy.as_deref() == Some("public")
            })
        });
    }

    // 3. 移除空的segments
    filtered.segments.retain(|segment| !segment.items.is_empty());

    tracing::debug!(
        "Filtered context for collab: {} segments, {} total items",
        filtered.segments.len(),
        filtered.segments.iter().map(|s| s.items.len()).sum::<usize>()
    );

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_context::types::{Anchor, BundleItem, BundleSegment, BudgetSummary, ContextBundle, ExplainBundle, Partition, PromptBundle};
    use soulseed_agi_core_models::{AccessClass, EnvelopeId, TenantId};

    fn dummy_anchor(access_class: AccessClass) -> Anchor {
        Anchor {
            tenant_id: TenantId::new(1),
            envelope_id: EnvelopeId::new_v4(),
            config_snapshot_hash: "test".into(),
            config_snapshot_version: 1,
            session_id: None,
            sequence_number: None,
            access_class,
            provenance: None,
            schema_v: 1,
            scenario: None,
            supersedes: None,
            superseded_by: None,
        }
    }

    fn dummy_explain_bundle() -> ExplainBundle {
        ExplainBundle {
            reasons: vec![],
            degradation_reason: None,
            indices_used: vec![],
            query_hash: None,
        }
    }

    fn dummy_budget_summary() -> BudgetSummary {
        BudgetSummary {
            target_tokens: 1000,
            projected_tokens: 800,
        }
    }

    fn dummy_bundle_item(id: &str, access_policy: Option<&str>) -> BundleItem {
        let evidence_ptrs = if let Some(policy) = access_policy {
            vec![EvidencePointer {
                uri: format!("soul://context/{}", id),
                digest_sha256: Some("abc123".into()),
                media_type: Some("text/plain".into()),
                blob_ref: None,
                span: None,
                access_policy: Some(policy.into()),
            }]
        } else {
            vec![]
        };

        BundleItem {
            ci_id: id.into(),
            partition: Partition::P4Dialogue,
            summary_level: None,
            tokens: 100,
            score_scaled: 100,
            ts_ms: 1000,
            digests: Default::default(),
            typ: Some("test".into()),
            why_included: None,
            score_stats: Default::default(),
            supersedes: None,
            evidence_ptrs,
        }
    }

    #[test]
    fn test_filter_context_for_collab_sets_access_class_to_public() {
        let context = ContextBundle {
            anchor: dummy_anchor(AccessClass::Restricted),
            schema_v: 1,
            version: 1,
            segments: vec![],
            explain: dummy_explain_bundle(),
            budget: dummy_budget_summary(),
            prompt: PromptBundle::default(),
        };

        let filtered = filter_context_for_collab(&context);

        assert_eq!(filtered.anchor.access_class, AccessClass::Public);
    }

    #[test]
    fn test_filter_context_for_collab_keeps_public_items() {
        let context = ContextBundle {
            anchor: dummy_anchor(AccessClass::Internal),
            schema_v: 1,
            version: 1,
            segments: vec![
                BundleSegment {
                    partition: Partition::P4Dialogue,
                    items: vec![
                        dummy_bundle_item("1", Some("public")),
                        dummy_bundle_item("2", Some("public")),
                    ],
                },
            ],
            explain: dummy_explain_bundle(),
            budget: dummy_budget_summary(),
            prompt: PromptBundle::default(),
        };

        let filtered = filter_context_for_collab(&context);

        assert_eq!(filtered.segments.len(), 1);
        assert_eq!(filtered.segments[0].items.len(), 2);
    }

    #[test]
    fn test_filter_context_for_collab_removes_restricted_items() {
        let context = ContextBundle {
            anchor: dummy_anchor(AccessClass::Internal),
            schema_v: 1,
            version: 1,
            segments: vec![
                BundleSegment {
                    partition: Partition::P4Dialogue,
                    items: vec![
                        dummy_bundle_item("1", Some("public")),
                        dummy_bundle_item("2", Some("restricted")),
                        dummy_bundle_item("3", Some("internal")),
                    ],
                },
            ],
            explain: dummy_explain_bundle(),
            budget: dummy_budget_summary(),
            prompt: PromptBundle::default(),
        };

        let filtered = filter_context_for_collab(&context);

        assert_eq!(filtered.segments.len(), 1);
        assert_eq!(filtered.segments[0].items.len(), 1);
        assert_eq!(filtered.segments[0].items[0].ci_id, "1");
    }

    #[test]
    fn test_filter_context_for_collab_keeps_items_without_evidence_ptrs() {
        let context = ContextBundle {
            anchor: dummy_anchor(AccessClass::Internal),
            schema_v: 1,
            version: 1,
            segments: vec![
                BundleSegment {
                    partition: Partition::P4Dialogue,
                    items: vec![
                        dummy_bundle_item("1", None), // No evidence_ptr
                        dummy_bundle_item("2", Some("public")),
                    ],
                },
            ],
            explain: dummy_explain_bundle(),
            budget: dummy_budget_summary(),
            prompt: PromptBundle::default(),
        };

        let filtered = filter_context_for_collab(&context);

        assert_eq!(filtered.segments.len(), 1);
        assert_eq!(filtered.segments[0].items.len(), 2);
    }

    #[test]
    fn test_filter_context_for_collab_removes_empty_segments() {
        let context = ContextBundle {
            anchor: dummy_anchor(AccessClass::Internal),
            schema_v: 1,
            version: 1,
            segments: vec![
                BundleSegment {
                    partition: Partition::P4Dialogue,
                    items: vec![
                        dummy_bundle_item("1", Some("restricted")),
                        dummy_bundle_item("2", Some("internal")),
                    ],
                },
                BundleSegment {
                    partition: Partition::P3WorkingDelta,
                    items: vec![
                        dummy_bundle_item("3", Some("public")),
                    ],
                },
            ],
            explain: dummy_explain_bundle(),
            budget: dummy_budget_summary(),
            prompt: PromptBundle::default(),
        };

        let filtered = filter_context_for_collab(&context);

        // First segment should be removed (all items filtered out)
        // Second segment should remain (has public item)
        assert_eq!(filtered.segments.len(), 1);
        assert_eq!(filtered.segments[0].partition, Partition::P3WorkingDelta);
        assert_eq!(filtered.segments[0].items.len(), 1);
        assert_eq!(filtered.segments[0].items[0].ci_id, "3");
    }
}

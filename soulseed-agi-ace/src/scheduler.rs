use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::budget::DegradationStrategy;
use crate::errors::AceError;
use crate::types::{
    BudgetSnapshot, CycleLane, CycleSchedule, CycleStatus, ScheduleOutcome, new_cycle_id,
};
use blake3::Hasher;
use serde_json::{Value, json, to_value};
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, AwarenessEvent, AwarenessEventType, AwarenessFork,
};
use soulseed_agi_core_models::{AwarenessCycleId, EventId};
use soulseed_agi_dfr::types::RouterDecision;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct SchedulerConfig {
    pub max_pending_per_tenant: usize,
    pub allow_parallel_lanes: bool,
    pub clarify_round_limit: u32,
    pub clarify_wait_limit_ms: u64,
    pub clarify_queue_threshold: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_per_tenant: 8,
            allow_parallel_lanes: false,
            clarify_round_limit: 3,
            clarify_wait_limit_ms: 60_000,
            clarify_queue_threshold: 4,
        }
    }
}

#[derive(Default)]
struct SchedulerState {
    pending: HashMap<u64, VecDeque<CycleSchedule>>,
    active: HashMap<u64, CycleLane>,
    clarify_stats: HashMap<u64, ClarifyStats>,
    last_fork: HashMap<u64, AwarenessFork>,
    status: HashMap<u64, CycleStatus>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClarifyStats {
    pub rounds: u32,
    pub first_enqueued_at: OffsetDateTime,
}

/// Clarify闸门降级评估结果
#[derive(Clone, Debug)]
pub struct ClarifyGateResult {
    pub should_accept: bool,
    pub degradation_strategy: Option<DegradationStrategy>,
    pub reason: Option<String>,
    pub metrics: ClarifyMetrics,
}

#[derive(Clone, Debug)]
pub struct ClarifyMetrics {
    pub rounds: u32,
    pub wait_ms: u64,
    pub queue_depth: usize,
    pub rounds_ratio: f32,
    pub wait_ratio: f32,
    pub queue_ratio: f32,
}

#[derive(Clone)]
pub struct CycleScheduler {
    cfg: SchedulerConfig,
    state: Arc<Mutex<SchedulerState>>,
}

impl CycleScheduler {
    pub fn new(cfg: SchedulerConfig) -> Self {
        Self {
            cfg,
            state: Arc::new(Mutex::new(SchedulerState::default())),
        }
    }

    /// 评估Clarify闸门状态并返回降级建议
    pub fn evaluate_clarify_gate(
        &self,
        _tenant: u64,
        predicted_queue: usize,
        stats: &ClarifyStats,
        now: OffsetDateTime,
    ) -> ClarifyGateResult {
        let waited_ms = (now - stats.first_enqueued_at)
            .whole_milliseconds()
            .max(0) as u64;
        let predicted_rounds = stats.rounds.saturating_add(1);

        // 计算各维度使用率
        let rounds_ratio = predicted_rounds as f32 / self.cfg.clarify_round_limit as f32;
        let wait_ratio = waited_ms as f32 / self.cfg.clarify_wait_limit_ms as f32;
        let queue_ratio = predicted_queue as f32 / self.cfg.clarify_queue_threshold as f32;

        // 取最高使用率作为整体评估指标
        let max_ratio = rounds_ratio.max(wait_ratio).max(queue_ratio);

        let metrics = ClarifyMetrics {
            rounds: predicted_rounds,
            wait_ms: waited_ms,
            queue_depth: predicted_queue,
            rounds_ratio,
            wait_ratio,
            queue_ratio,
        };

        // 完全超限，拒绝
        if max_ratio >= 1.0 {
            return ClarifyGateResult {
                should_accept: false,
                degradation_strategy: Some(DegradationStrategy::Reject),
                reason: Some("clarify_exhausted".into()),
                metrics,
            };
        }

        // 根据使用率渐进式降级（类似P1-3预算降级树）
        let degradation_strategy = if max_ratio >= 0.95 {
            // 95%: 请求人工决策
            Some(DegradationStrategy::AskHumanDecision)
        } else if max_ratio >= 0.85 {
            // 85%: 暂停等待或转协同
            Some(DegradationStrategy::Pause)
        } else if max_ratio >= 0.75 {
            // 75%: 转工具执行
            Some(DegradationStrategy::TransferToTool)
        } else if max_ratio >= 0.60 {
            // 60%: 提供保守答案
            Some(DegradationStrategy::Conservative)
        } else {
            // < 60%: 正常执行
            None
        };

        let reason = if degradation_strategy.is_some() {
            Some(format!(
                "clarify_degrading: rounds={}/{}, wait={}ms/{}ms, queue={}/{}",
                predicted_rounds,
                self.cfg.clarify_round_limit,
                waited_ms,
                self.cfg.clarify_wait_limit_ms,
                predicted_queue,
                self.cfg.clarify_queue_threshold
            ))
        } else {
            None
        };

        ClarifyGateResult {
            should_accept: true,
            degradation_strategy,
            reason,
            metrics,
        }
    }

    pub fn schedule(
        &self,
        decision: RouterDecision,
        budget: BudgetSnapshot,
        parent_cycle_id: Option<AwarenessCycleId>,
        collab_scope_id: Option<String>,
    ) -> Result<ScheduleOutcome, AceError> {
        let plan = &decision.plan;
        let plan_cycle_id = plan.cycle_id;
        let anchor = plan.anchor.clone();
        let tenant = anchor.tenant_id.into_inner();
        let lane: CycleLane = plan.fork.into();
        let now = OffsetDateTime::now_utc();

        if !self.cfg.allow_parallel_lanes {
            let guard = self.state.lock().unwrap();
            if let Some(active_lane) = guard.active.get(&tenant) {
                if matches!(
                    (active_lane, &lane),
                    (CycleLane::Clarify, CycleLane::Clarify)
                ) {
                    return Ok(ScheduleOutcome {
                        accepted: false,
                        reason: Some("clarify_lane_busy".into()),
                        cycle: None,
                        awareness_events: Vec::new(),
                        fork_weights: None,
                    });
                }
            }
        }

        let mut guard = self.state.lock().unwrap();
        let queue_len = {
            let queue = guard.pending.entry(tenant).or_default();
            if !self.cfg.allow_parallel_lanes && matches!(lane, CycleLane::Clarify) {
                if queue.iter().any(|c| matches!(c.lane, CycleLane::Clarify)) {
                    return Ok(ScheduleOutcome {
                        accepted: false,
                        reason: Some("clarify_lane_busy".into()),
                        cycle: None,
                        awareness_events: Vec::new(),
                        fork_weights: None,
                    });
                }
            }
            queue.len()
        };

        if queue_len >= self.cfg.max_pending_per_tenant {
            return Ok(ScheduleOutcome {
                accepted: false,
                reason: Some("pending_limit".into()),
                cycle: None,
                awareness_events: Vec::new(),
                fork_weights: None,
            });
        }

        // Clarify闸门检查和降级评估
        let mut degradation_strategy = None;
        if matches!(lane, CycleLane::Clarify) {
            let queue = guard.pending.entry(tenant).or_default();
            let predicted_queue = queue
                .iter()
                .filter(|c| matches!(c.lane, CycleLane::Clarify))
                .count()
                + 1;
            let stats_entry = guard.clarify_stats.entry(tenant).or_insert(ClarifyStats {
                rounds: 0,
                first_enqueued_at: now,
            });
            if stats_entry.rounds == 0 {
                stats_entry.first_enqueued_at = now;
            }

            // 使用降级树评估Clarify gate状态
            let gate_result = self.evaluate_clarify_gate(tenant, predicted_queue, stats_entry, now);

            if !gate_result.should_accept {
                let event = build_degradation_event_with_strategy(
                    &anchor,
                    &gate_result.metrics,
                    gate_result.degradation_strategy.as_ref(),
                );
                return Ok(ScheduleOutcome {
                    accepted: false,
                    reason: gate_result.reason,
                    cycle: None,
                    awareness_events: vec![event],
                    fork_weights: None,
                });
            }

            // 即使接受，也可能有降级建议 - 保存到CycleSchedule中供后续使用
            if let Some(strategy) = &gate_result.degradation_strategy {
                tracing::warn!(
                    "Clarify gate degradation: tenant={}, strategy={:?}, metrics={:?}",
                    tenant,
                    strategy,
                    gate_result.metrics
                );
                degradation_strategy = Some(strategy.clone());
            }

            stats_entry.rounds = gate_result.metrics.rounds;
        }

        let prev_fork = guard.last_fork.get(&tenant).copied();
        let collab_scope_ref = collab_scope_id.as_deref();
        let decision_events = build_decision_events(
            &anchor,
            plan_cycle_id,
            &decision,
            &lane,
            prev_fork,
            parent_cycle_id,
            collab_scope_ref,
        );
        let explain_fingerprint = fingerprint_decision(&decision);

        // 生成四分叉权重对比（在decision被move之前）
        let fork_weights = Some(generate_fork_weights(&decision, &budget));

        let initial_status = if matches!(lane, CycleLane::SelfReason) {
            CycleStatus::Running
        } else {
            CycleStatus::AwaitingExternal
        };

        let cycle = CycleSchedule {
            cycle_id: plan_cycle_id,
            lane: lane.clone(),
            anchor: anchor.clone(),
            budget: budget.clone(),
            created_at: now,
            router_decision: decision,
            decision_events: decision_events.clone(),
            explain_fingerprint: explain_fingerprint.clone(),
            status: initial_status,
            parent_cycle_id,
            collab_scope_id: collab_scope_id.clone(),
            degradation_strategy,
        };

        guard
            .pending
            .entry(tenant)
            .or_default()
            .push_back(cycle.clone());
        guard.last_fork.insert(tenant, lane.as_fork());
        guard.status.insert(plan_cycle_id.as_u64(), initial_status);

        Ok(ScheduleOutcome {
            accepted: true,
            reason: None,
            cycle: Some(cycle),
            awareness_events: decision_events,
            fork_weights,
        })
    }

    pub fn start_next(&self, tenant: u64) -> Option<CycleSchedule> {
        let mut guard = self.state.lock().unwrap();
        let queue = guard.pending.get_mut(&tenant)?;
        let queue_len = queue.len();
        tracing::info!("Scheduler::start_next: tenant={}, queue_len={}", tenant, queue_len);
        let mut cycle = queue.pop_front()?;
        let current_status = guard.status.get(&cycle.cycle_id.as_u64()).copied();
        tracing::info!("Scheduler::start_next: popped cycle_id={} (u64={}), current_status={:?}",
            cycle.cycle_id, cycle.cycle_id.as_u64(), current_status);
        match current_status {
            Some(CycleStatus::AwaitingExternal | CycleStatus::Suspended) => {
                cycle.status = current_status.unwrap();
            }
            Some(status) if status != CycleStatus::Running => {
                cycle.status = status;
            }
            _ => {
                cycle.status = CycleStatus::Running;
                guard
                    .status
                    .insert(cycle.cycle_id.as_u64(), CycleStatus::Running);
            }
        }
        guard.active.insert(tenant, cycle.lane.clone());
        Some(cycle)
    }

    pub fn finish(&self, tenant: u64, cycle_id: AwarenessCycleId, status: CycleStatus) {
        let mut guard = self.state.lock().unwrap();
        guard.active.remove(&tenant);
        guard.status.insert(cycle_id.as_u64(), status);
        if matches!(status, CycleStatus::Completed | CycleStatus::Failed) {
            guard.status.remove(&cycle_id.as_u64());
        }
        if let Some(stats) = guard.clarify_stats.get_mut(&tenant) {
            stats.rounds = stats.rounds.saturating_sub(1);
            if stats.rounds == 0 {
                guard.clarify_stats.remove(&tenant);
            }
        }
    }

    /// 应用降级策略 - 根据degradation_strategy生成awareness事件
    ///
    /// 用于在HITL超时或其他降级场景时，记录降级决策和触发相应的降级行为
    pub fn apply_degradation_strategy(
        &self,
        schedule: &CycleSchedule,
        reason: &str,
    ) -> Vec<soulseed_agi_core_models::awareness::AwarenessEvent> {
        let Some(strategy) = &schedule.degradation_strategy else {
            return Vec::new();
        };

        use uuid::Uuid;

        let event = soulseed_agi_core_models::awareness::AwarenessEvent {
            anchor: schedule.anchor.clone(),
            event_id: soulseed_agi_core_models::EventId::new(Uuid::now_v7().as_u128() as u64),
            event_type: soulseed_agi_core_models::awareness::AwarenessEventType::SyncPointReported,
            occurred_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
            awareness_cycle_id: schedule.cycle_id,
            parent_cycle_id: schedule.parent_cycle_id,
            collab_scope_id: schedule.collab_scope_id.clone(),
            barrier_id: None,
            env_mode: None,
            inference_cycle_sequence: 1,
            degradation_reason: None,
            payload: serde_json::json!({
                "type": "degradation_applied",
                "strategy": format!("{:?}", strategy),
                "lane": format!("{:?}", schedule.lane),
                "cycle_id": schedule.cycle_id.as_u64(),
                "reason": reason,
            }),
        };

        tracing::info!(
            "Applying degradation strategy: cycle={}, lane={:?}, strategy={:?}, reason={}",
            schedule.cycle_id.as_u64(),
            schedule.lane,
            strategy,
            reason
        );

        vec![event]
    }

    pub fn mark_status(&self, cycle_id: AwarenessCycleId, status: CycleStatus) {
        let mut guard = self.state.lock().unwrap();
        guard.status.insert(cycle_id.as_u64(), status);
    }

    pub fn status_of(&self, cycle_id: AwarenessCycleId) -> Option<CycleStatus> {
        let guard = self.state.lock().unwrap();
        guard.status.get(&cycle_id.as_u64()).copied()
    }

    /// 将已存在的周期重新加入pending队列（用于从数据库恢复）
    pub fn reschedule(&self, cycle: CycleSchedule) {
        let tenant = cycle.anchor.tenant_id.into_inner();
        tracing::info!("Scheduler::reschedule: cycle_id={} (u64={}), tenant={}, status={:?}",
            cycle.cycle_id, cycle.cycle_id.as_u64(), tenant, cycle.status);
        let mut guard = self.state.lock().unwrap();
        // 记录周期状态
        guard.status.insert(cycle.cycle_id.as_u64(), cycle.status);
        // 加入pending队列
        guard.pending.entry(tenant).or_default().push_back(cycle);
    }
}

fn build_decision_events(
    anchor: &AwarenessAnchor,
    cycle_id: AwarenessCycleId,
    decision: &RouterDecision,
    lane: &CycleLane,
    previous: Option<AwarenessFork>,
    parent_cycle_id: Option<AwarenessCycleId>,
    collab_scope_id: Option<&str>,
) -> Vec<AwarenessEvent> {
    let mut events = Vec::new();
    let mut next_event_id = cycle_id.as_u64();
    let occurred_at_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let mut degradation = decision.decision_path.degradation_reason;
    if degradation.is_none() {
        degradation = map_degradation(decision.plan.explain.degradation_reason.as_deref());
    }

    let plan_value = to_value(&decision.decision_path.plan).unwrap_or_else(|_| Value::Null);
    let rationale_value =
        to_value(&decision.decision_path.rationale).unwrap_or_else(|_| Value::Null);
    let budget_plan_value =
        to_value(&decision.decision_path.budget_plan).unwrap_or_else(|_| Value::Null);
    let decision_path_value = to_value(&decision.decision_path).unwrap_or_else(|_| Value::Null);
    let route_plan_value = to_value(&decision.plan).unwrap_or_else(|_| Value::Null);
    let collab_scope_string = collab_scope_id.map(|s| s.to_string());

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::AwarenessCycleStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: None,
        payload: json!({
            "lane": format!("{:?}", lane),
            "context_digest": decision.context_digest,
            "router_digest": decision.plan.explain.router_digest,
            "router_config_digest": decision.plan.explain.router_config_digest,
            "routing_seed": decision.plan.explain.routing_seed,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::InferenceCycleStarted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: None,
        payload: json!({
            "lane": format!("{:?}", lane),
            "ic_sequence": decision.decision_path.inference_cycle_sequence,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::AssessmentProduced,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: degradation,
        payload: json!({
            "lane": format!("{:?}", lane),
            "plan": plan_value.clone(),
            "rationale": rationale_value,
            "budget_plan": budget_plan_value,
            "confidence": decision.decision_path.confidence,
        }),
    });
    next_event_id += 1;

    events.push(AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(next_event_id),
        event_type: AwarenessEventType::DecisionRouted,
        occurred_at_ms,
        awareness_cycle_id: cycle_id,
        parent_cycle_id,
        collab_scope_id: collab_scope_string.clone(),
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
        degradation_reason: degradation,
        payload: json!({
            "fork": format!("{:?}", lane),
            "decision_path": decision_path_value,
            "route_plan": route_plan_value,
            "rejected": decision.rejected,
        }),
    });
    next_event_id += 1;

    if let Some(prev) = previous {
        if prev != lane.as_fork() {
            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId::from_raw_unchecked(next_event_id),
                event_type: AwarenessEventType::RouteReconsidered,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id,
                collab_scope_id: collab_scope_string.clone(),
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
                degradation_reason: degradation,
                payload: json!({
                    "from": format!("{:?}", CycleLane::from(prev)),
                    "to": format!("{:?}", lane),
                    "rejected": decision.rejected,
                }),
            });
            next_event_id += 1;

            events.push(AwarenessEvent {
                anchor: anchor.clone(),
                event_id: EventId::from_raw_unchecked(next_event_id),
                event_type: AwarenessEventType::RouteSwitched,
                occurred_at_ms,
                awareness_cycle_id: cycle_id,
                parent_cycle_id,
                collab_scope_id: collab_scope_string,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: decision.decision_path.inference_cycle_sequence,
                degradation_reason: degradation,
                payload: json!({
                    "from": format!("{:?}", CycleLane::from(prev)),
                    "to": format!("{:?}", lane),
                    "router_digest": decision.plan.explain.router_digest,
                }),
            });
        }
    }

    events
}

fn fingerprint_decision(decision: &RouterDecision) -> Option<String> {
    let mut hasher = Hasher::new();
    let path_bytes = serde_json::to_vec(&decision.decision_path).ok()?;
    hasher.update(&path_bytes);
    if let Ok(plan_bytes) = serde_json::to_vec(&decision.plan.explain) {
        hasher.update(&plan_bytes);
    }
    Some(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn build_degradation_event_with_strategy(
    anchor: &AwarenessAnchor,
    metrics: &ClarifyMetrics,
    strategy: Option<&DegradationStrategy>,
) -> AwarenessEvent {
    let cycle_id = new_cycle_id();
    AwarenessEvent {
        anchor: anchor.clone(),
        event_id: EventId::from_raw_unchecked(cycle_id.as_u64()),
        event_type: AwarenessEventType::DecisionRouted,
        occurred_at_ms: OffsetDateTime::now_utc().unix_timestamp() * 1000,
        awareness_cycle_id: cycle_id,
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: Some(AwarenessDegradationReason::ClarifyExhausted),
        payload: json!({
            "lane": "clarify",
            "reason": "clarify_exhausted",
            "rounds": metrics.rounds,
            "wait_ms": metrics.wait_ms,
            "queue_depth": metrics.queue_depth,
            "rounds_ratio": metrics.rounds_ratio,
            "wait_ratio": metrics.wait_ratio,
            "queue_ratio": metrics.queue_ratio,
            "degradation_strategy": strategy.as_ref().map(|s| format!("{:?}", s)),
        }),
    }
}

// 保留旧函数用于向后兼容
#[allow(dead_code)]
fn build_degradation_event(
    anchor: &AwarenessAnchor,
    rounds: u32,
    wait_ms: u64,
    queue_depth: usize,
) -> AwarenessEvent {
    let metrics = ClarifyMetrics {
        rounds,
        wait_ms,
        queue_depth,
        rounds_ratio: 1.0,
        wait_ratio: 1.0,
        queue_ratio: 1.0,
    };
    build_degradation_event_with_strategy(anchor, &metrics, Some(&DegradationStrategy::Reject))
}

fn map_degradation(reason: Option<&str>) -> Option<AwarenessDegradationReason> {
    match reason {
        Some("clarify_exhausted") => Some(AwarenessDegradationReason::ClarifyExhausted),
        Some("graph_degraded") => Some(AwarenessDegradationReason::GraphDegraded),
        Some("envctx_degraded") => Some(AwarenessDegradationReason::EnvctxDegraded),
        Some("privacy_blocked") => Some(AwarenessDegradationReason::PrivacyBlocked),
        Some("budget_tokens") => Some(AwarenessDegradationReason::BudgetTokens),
        Some("budget_walltime") => Some(AwarenessDegradationReason::BudgetWalltime),
        Some("budget_external_cost") => Some(AwarenessDegradationReason::BudgetExternalCost),
        Some("invalid_plan") => Some(AwarenessDegradationReason::InvalidPlan),
        _ => None,
    }
}

/// 生成四分叉权重对比
///
/// 基于RouterDecision和BudgetSnapshot，使用启发式算法计算所有四个分叉的权重。
/// 这为调试和解释路由决策提供了透明度。
///
/// # Arguments
/// * `decision` - 路由决策
/// * `budget` - 预算快照
///
/// # Returns
/// 包含所有四个分叉权重和贡献度分解的ForkWeightsComparison
fn generate_fork_weights(
    decision: &RouterDecision,
    budget: &BudgetSnapshot,
) -> crate::types::ForkWeightsComparison {
    use crate::types::{ForkWeightsComparison, WeightContribution};
    use soulseed_agi_core_models::awareness::DecisionPlan;

    // 计算预算使用率（0.0-1.0）
    let token_ratio = if budget.tokens_allowed > 0 {
        budget.tokens_spent as f32 / budget.tokens_allowed as f32
    } else {
        0.0
    };

    // 基于预算约束的权重贡献
    let budget_factor = (1.0 - token_ratio).max(0.0);

    // 基于当前选择的分叉，给它最高权重
    let (clarify_base, tool_base, self_reason_base, collab_base) = match &decision.plan.decision_plan {
        DecisionPlan::Clarify { .. } => (0.8, 0.4, 0.3, 0.2),
        DecisionPlan::Tool { .. } => (0.3, 0.8, 0.4, 0.3),
        DecisionPlan::SelfReason { .. } => (0.2, 0.3, 0.8, 0.4),
        DecisionPlan::Collab { .. } => (0.2, 0.3, 0.4, 0.8),
    };

    // 应用预算因子调整权重
    let clarify_weight = (clarify_base * 0.7 + budget_factor * 0.3).clamp(0.0, 1.0);
    let tool_weight = (tool_base * 0.7 + budget_factor * 0.3).clamp(0.0, 1.0);
    let self_reason_weight = (self_reason_base * 0.7 + budget_factor * 0.3).clamp(0.0, 1.0);
    let collab_weight = (collab_base * 0.7 + budget_factor * 0.3).clamp(0.0, 1.0);

    // 生成贡献度分解
    let contribution = WeightContribution {
        context_relevance: 0.6, // 简化：假设上下文相关性为0.6
        budget_constraint: budget_factor,
        tool_availability: 0.7, // 简化：假设工具可用性为0.7
        collab_need: 0.4,       // 简化：假设协同需求为0.4
        historical_success: 0.5, // 简化：假设历史成功率为0.5
    };

    ForkWeightsComparison {
        clarify_weight,
        tool_weight,
        self_reason_weight,
        collab_weight,
        contribution_breakdown: Some(contribution),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn create_test_scheduler() -> CycleScheduler {
        let cfg = SchedulerConfig {
            max_pending_per_tenant: 8,
            allow_parallel_lanes: false,
            clarify_round_limit: 10,
            clarify_wait_limit_ms: 10000,
            clarify_queue_threshold: 100,
        };
        CycleScheduler::new(cfg)
    }

    #[test]
    fn test_clarify_gate_normal_acceptance() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 3, // 30% of limit
            first_enqueued_at: now - time::Duration::milliseconds(2000), // 20% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            30, // 30% of queue threshold
            &stats,
            now,
        );

        assert!(result.should_accept, "Should accept at < 60% usage");
        assert!(result.degradation_strategy.is_none(), "No degradation at < 60%");
        assert!(result.reason.is_none(), "No degradation reason at < 60%");
    }

    #[test]
    fn test_clarify_gate_conservative_degradation() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 6, // 60% of limit
            first_enqueued_at: now - time::Duration::milliseconds(5000), // 50% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            60, // 60% of queue threshold
            &stats,
            now,
        );

        assert!(result.should_accept, "Should accept but with degradation");
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::Conservative),
            "Should use Conservative strategy at 60-75%"
        );
        assert!(result.reason.is_some());
        assert!(result.reason.unwrap().contains("clarify_degrading"));
    }

    #[test]
    fn test_clarify_gate_transfer_to_tool() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 6, // predicted_rounds = 7, 70% of limit
            first_enqueued_at: now - time::Duration::milliseconds(7600), // 76% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            75, // 75% of queue threshold
            &stats,
            now,
        );

        assert!(result.should_accept, "Should accept but with degradation");
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::TransferToTool),
            "Should use TransferToTool strategy at 75-85%"
        );
        assert!(result.reason.is_some());
        assert!(result.reason.unwrap().contains("clarify_degrading"));
    }

    #[test]
    fn test_clarify_gate_pause_degradation() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 7, // predicted_rounds = 8, 80% of limit
            first_enqueued_at: now - time::Duration::milliseconds(8600), // 86% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            85, // 85% of queue threshold
            &stats,
            now,
        );

        assert!(result.should_accept, "Should accept but with degradation");
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::Pause),
            "Should use Pause strategy at 85-95%"
        );
        assert!(result.reason.is_some());
        assert!(result.reason.unwrap().contains("clarify_degrading"));
    }

    #[test]
    fn test_clarify_gate_ask_human_decision() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 8, // predicted_rounds = 9, 90% of limit
            first_enqueued_at: now - time::Duration::milliseconds(9600), // 96% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            95, // 95% of queue threshold
            &stats,
            now,
        );

        assert!(result.should_accept, "Should accept but with degradation");
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::AskHumanDecision),
            "Should use AskHumanDecision strategy at 95-100%"
        );
        assert!(result.reason.is_some());
        assert!(result.reason.unwrap().contains("clarify_degrading"));
    }

    #[test]
    fn test_clarify_gate_rejection() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 10, // 100% of limit
            first_enqueued_at: now - time::Duration::milliseconds(10000), // 100% of limit
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            100, // 100% of queue threshold
            &stats,
            now,
        );

        assert!(!result.should_accept, "Should reject at >= 100% usage");
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::Reject),
            "Should use Reject strategy when >= 100%"
        );
        assert!(result.reason.is_some());
        assert_eq!(result.reason.unwrap(), "clarify_exhausted");
    }

    #[test]
    fn test_clarify_gate_multi_dimensional_rounds() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 6, // predicted_rounds = 7, 70% - but we want 80% to trigger TransferToTool
            first_enqueued_at: now - time::Duration::milliseconds(5000), // 50%
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            80, // 80% - highest, triggers TransferToTool
            &stats,
            now,
        );

        assert!(result.should_accept);
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::TransferToTool),
            "Should use strategy based on max dimension (queue at 80%)"
        );
        assert_eq!(result.metrics.queue_ratio, 0.8);
    }

    #[test]
    fn test_clarify_gate_multi_dimensional_wait_time() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 3, // 30%
            first_enqueued_at: now - time::Duration::milliseconds(9000), // 90% - highest
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            40, // 40%
            &stats,
            now,
        );

        assert!(result.should_accept);
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::Pause),
            "Should use strategy based on max dimension (wait time at 90%)"
        );
        assert_eq!(result.metrics.wait_ratio, 0.9);
    }

    #[test]
    fn test_clarify_gate_multi_dimensional_queue_depth() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 4, // 40%
            first_enqueued_at: now - time::Duration::milliseconds(3000), // 30%
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            96, // 96% - highest
            &stats,
            now,
        );

        assert!(result.should_accept);
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::AskHumanDecision),
            "Should use strategy based on max dimension (queue at 96%)"
        );
        assert_eq!(result.metrics.queue_ratio, 0.96);
    }

    #[test]
    fn test_clarify_gate_metrics_calculation() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 5, // 50%
            first_enqueued_at: now - time::Duration::milliseconds(7500), // 75%
        };

        let result = scheduler.evaluate_clarify_gate(
            1,
            80, // 80%
            &stats,
            now,
        );

        // Verify metrics are calculated correctly
        assert_eq!(result.metrics.rounds, 6); // predicted_rounds = 5 + 1
        assert_eq!(result.metrics.rounds_ratio, 0.6); // 6/10
        assert_eq!(result.metrics.wait_ms, 7500);
        assert_eq!(result.metrics.wait_ratio, 0.75); // 7500/10000
        assert_eq!(result.metrics.queue_depth, 80);
        assert_eq!(result.metrics.queue_ratio, 0.8); // 80/100
        // Verify degradation is triggered at 80% (max ratio)
        assert_eq!(
            result.degradation_strategy,
            Some(DegradationStrategy::TransferToTool)
        );
    }

    #[test]
    fn test_clarify_gate_edge_case_zero_wait() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 0,
            first_enqueued_at: now, // Just enqueued
        };

        let result = scheduler.evaluate_clarify_gate(1, 0, &stats, now);

        assert!(result.should_accept);
        assert!(result.degradation_strategy.is_none());
        assert_eq!(result.metrics.wait_ms, 0);
        assert_eq!(result.metrics.wait_ratio, 0.0);
    }

    #[test]
    fn test_clarify_gate_edge_case_negative_wait() {
        let scheduler = create_test_scheduler();
        let now = OffsetDateTime::now_utc();
        let stats = ClarifyStats {
            rounds: 2,
            first_enqueued_at: now + time::Duration::milliseconds(1000), // Future time (shouldn't happen)
        };

        let result = scheduler.evaluate_clarify_gate(1, 20, &stats, now);

        // Should handle gracefully with max(0)
        assert!(result.should_accept);
        assert_eq!(result.metrics.wait_ms, 0);
        assert_eq!(result.metrics.wait_ratio, 0.0);
    }

    #[test]
    fn test_fork_weights_comparison_struct() {
        use crate::types::{ForkWeightsComparison, WeightContribution};

        let weights = ForkWeightsComparison {
            clarify_weight: 0.8,
            tool_weight: 0.6,
            self_reason_weight: 0.4,
            collab_weight: 0.3,
            contribution_breakdown: Some(WeightContribution {
                context_relevance: 0.7,
                budget_constraint: 0.5,
                tool_availability: 0.8,
                collab_need: 0.4,
                historical_success: 0.6,
            }),
        };

        // Verify structure
        assert!(weights.clarify_weight > weights.tool_weight);
        assert!(weights.tool_weight > weights.self_reason_weight);
        assert!(weights.self_reason_weight > weights.collab_weight);
        assert!(weights.contribution_breakdown.is_some());

        let contribution = weights.contribution_breakdown.unwrap();
        assert_eq!(contribution.context_relevance, 0.7);
        assert_eq!(contribution.budget_constraint, 0.5);
    }
}

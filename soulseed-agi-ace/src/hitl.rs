use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::{AwarenessCycleId, TenantId};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HitlPriority {
    P0Critical,
    P1High,
    P2Medium,
    P3Low,
}

impl HitlPriority {
    fn rank(self) -> u8 {
        match self {
            HitlPriority::P0Critical => 0,
            HitlPriority::P1High => 1,
            HitlPriority::P2Medium => 2,
            HitlPriority::P3Low => 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HitlInjection {
    pub injection_id: Uuid,
    pub tenant_id: TenantId,
    pub priority: HitlPriority,
    pub author_role: String,
    pub submitted_at: OffsetDateTime,
    pub payload: Value,
}

impl HitlInjection {
    pub fn new(
        tenant_id: TenantId,
        priority: HitlPriority,
        author_role: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            injection_id: Uuid::now_v7(),
            tenant_id,
            priority,
            author_role: author_role.into(),
            submitted_at: OffsetDateTime::now_utc(),
            payload,
        }
    }

    pub fn with_submitted_at(mut self, submitted_at: OffsetDateTime) -> Self {
        self.submitted_at = submitted_at;
        self
    }
}

/// 延迟注入的触发条件
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// 当预算超出阈值时触发
    BudgetExceeded {
        /// 预算使用率阈值 (0.0-1.0)
        threshold: u8,
    },
    /// 当路由变更时触发
    RouteChanged,
    /// 当指定周期完成时触发
    CycleCompleted {
        cycle_id: AwarenessCycleId,
    },
    /// 当时间到期时触发（在expires_at字段中指定）
    TimeElapsed,
    /// 当任意SyncPoint完成时触发
    AnySyncPointCompleted,
}

/// 延迟注入 - 满足条件后才会注入到HITL队列
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeferredInjection {
    pub deferred_id: Uuid,
    pub injection: HitlInjection,
    pub trigger_condition: TriggerCondition,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

impl DeferredInjection {
    pub fn new(
        injection: HitlInjection,
        trigger_condition: TriggerCondition,
        ttl_seconds: i64,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            deferred_id: Uuid::now_v7(),
            injection,
            trigger_condition,
            created_at: now,
            expires_at: now + time::Duration::seconds(ttl_seconds),
        }
    }

    pub fn with_expires_at(mut self, expires_at: OffsetDateTime) -> Self {
        self.expires_at = expires_at;
        self
    }

    /// 检查是否已过期
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.expires_at
    }
}

#[derive(Clone, Debug)]
pub struct HitlQueueConfig {
    pub role_priority: HashMap<String, u8>,
}

impl Default for HitlQueueConfig {
    fn default() -> Self {
        let mut role_priority = HashMap::new();
        role_priority.insert("system".into(), 0);
        role_priority.insert("facilitator".into(), 1);
        role_priority.insert("owner".into(), 2);
        role_priority.insert("participant".into(), 3);
        role_priority.insert("guest".into(), 4);
        Self { role_priority }
    }
}

#[derive(Clone)]
pub struct HitlQueue {
    cfg: HitlQueueConfig,
    state: Arc<Mutex<QueueState>>,
    seq: Arc<AtomicU64>,
}

#[derive(Default)]
struct QueueState {
    heap: BinaryHeap<QueuedInjection>,
}

#[derive(Clone, Debug)]
struct QueuedInjection {
    priority_rank: u8,
    role_rank: u8,
    submitted_at: OffsetDateTime,
    seq: u64,
    injection: HitlInjection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClarifyQueueEntry {
    pub injection: HitlInjection,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClarifyAbortRecord {
    pub injection_id: Uuid,
    pub tenant_id: TenantId,
    pub authorized_by: String,
    pub reason: String,
    pub occurred_at: OffsetDateTime,
}

impl Ord for QueuedInjection {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority_rank
            .cmp(&self.priority_rank)
            .then_with(|| other.role_rank.cmp(&self.role_rank))
            .then_with(|| other.submitted_at.cmp(&self.submitted_at))
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for QueuedInjection {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueuedInjection {
    fn eq(&self, other: &Self) -> bool {
        self.injection.injection_id == other.injection.injection_id
    }
}

impl Eq for QueuedInjection {}

impl HitlQueue {
    pub fn new(cfg: HitlQueueConfig) -> Self {
        Self {
            cfg,
            state: Arc::new(Mutex::new(QueueState::default())),
            seq: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn enqueue(&self, mut injection: HitlInjection) {
        if injection.submitted_at == OffsetDateTime::UNIX_EPOCH {
            injection.submitted_at = OffsetDateTime::now_utc();
        }
        let priority_rank = injection.priority.rank();
        let role_rank = self.role_rank(&injection.author_role);
        let seq = self.seq.fetch_add(1, AtomicOrdering::Relaxed);
        let mut guard = self.state.lock().expect("hitl queue poisoned");
        guard.heap.push(QueuedInjection {
            priority_rank,
            role_rank,
            submitted_at: injection.submitted_at,
            seq,
            injection,
        });
    }

    pub fn peek_priority(&self) -> Option<HitlPriority> {
        let guard = self.state.lock().expect("hitl queue poisoned");
        guard.heap.peek().map(|item| item.injection.priority)
    }

    pub fn len(&self) -> usize {
        self.state.lock().expect("hitl queue poisoned").heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pop_ready(&self, active_tenants: &HashSet<u64>) -> Option<HitlInjection> {
        let mut guard = self.state.lock().expect("hitl queue poisoned");
        let mut skipped = Vec::new();
        while let Some(candidate) = guard.heap.pop() {
            let tenant_raw = candidate.injection.tenant_id.into_inner();
            let can_interrupt = !active_tenants.contains(&tenant_raw)
                || matches!(candidate.injection.priority, HitlPriority::P0Critical);
            if can_interrupt {
                for skipped_item in skipped {
                    guard.heap.push(skipped_item);
                }
                return Some(candidate.injection);
            }
            skipped.push(candidate);
        }
        for skipped_item in skipped {
            guard.heap.push(skipped_item);
        }
        None
    }

    pub fn pop_for_clarify(&self, active_tenants: &HashSet<u64>) -> Option<HitlInjection> {
        let mut guard = self.state.lock().expect("hitl queue poisoned");
        let mut skipped = Vec::new();
        while let Some(candidate) = guard.heap.pop() {
            let tenant_raw = candidate.injection.tenant_id.into_inner();
            let can_interrupt = !active_tenants.contains(&tenant_raw)
                || matches!(
                    candidate.injection.priority,
                    HitlPriority::P0Critical | HitlPriority::P1High
                );
            if can_interrupt {
                for skipped_item in skipped {
                    guard.heap.push(skipped_item);
                }
                return Some(candidate.injection);
            }
            skipped.push(candidate);
        }
        for skipped_item in skipped {
            guard.heap.push(skipped_item);
        }
        None
    }

    pub fn remove(&self, injection_id: Uuid) -> Option<HitlInjection> {
        let mut guard = self.state.lock().expect("hitl queue poisoned");
        let mut drained: Vec<_> = guard.heap.drain().collect();
        let mut removed = None;
        let mut rebuilt = BinaryHeap::new();
        while let Some(item) = drained.pop() {
            if item.injection.injection_id == injection_id {
                removed = Some(item.injection);
            } else {
                rebuilt.push(item);
            }
        }
        guard.heap = rebuilt;
        removed
    }

    pub fn topk_for_tenant(&self, tenant: u64, k: usize) -> Vec<ClarifyQueueEntry> {
        let guard = self.state.lock().expect("hitl queue poisoned");
        let mut snapshot = guard.heap.clone();
        let mut entries = Vec::new();
        while let Some(item) = snapshot.pop() {
            if item.injection.tenant_id.into_inner() != tenant {
                continue;
            }
            let reason = format!(
                "priority={:?},role_rank={},submitted_at_ms={}",
                item.injection.priority,
                item.role_rank,
                item.submitted_at.unix_timestamp() * 1000
            );
            entries.push(ClarifyQueueEntry {
                injection: item.injection.clone(),
                reason,
            });
            if entries.len() == k {
                break;
            }
        }
        entries
    }

    pub fn clear(&self) {
        self.state.lock().expect("hitl queue poisoned").heap.clear();
    }

    fn role_rank(&self, role: &str) -> u8 {
        let role_key = role.to_ascii_lowercase();
        self.cfg.role_priority.get(&role_key).copied().unwrap_or(5)
    }
}

#[derive(Clone)]
pub struct HitlService {
    queue: HitlQueue,
    active_tenants: Arc<Mutex<HashSet<u64>>>,
    abort_records: Arc<Mutex<Vec<ClarifyAbortRecord>>>,
}

impl HitlService {
    pub fn new(cfg: HitlQueueConfig) -> Self {
        Self {
            queue: HitlQueue::new(cfg),
            active_tenants: Arc::new(Mutex::new(HashSet::new())),
            abort_records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn enqueue(&self, injection: HitlInjection) {
        self.queue.enqueue(injection);
    }

    pub fn mark_cycle_active(&self, tenant: TenantId) {
        self.active_tenants
            .lock()
            .expect("hitl active set poisoned")
            .insert(tenant.into_inner());
    }

    pub fn clear_cycle_active(&self, tenant: TenantId) {
        self.active_tenants
            .lock()
            .expect("hitl active set poisoned")
            .remove(&tenant.into_inner());
    }

    pub fn next_ready(&self) -> Option<HitlInjection> {
        let active = self
            .active_tenants
            .lock()
            .expect("hitl active set poisoned")
            .clone();
        self.queue.pop_ready(&active)
    }

    pub fn next_ready_for_clarify(&self) -> Option<HitlInjection> {
        let active = self
            .active_tenants
            .lock()
            .expect("hitl active set poisoned")
            .clone();
        self.queue.pop_for_clarify(&active)
    }

    pub fn pending_for(&self, tenant: TenantId) -> usize {
        let tenant_id = tenant.into_inner();
        let guard = self.queue.state.lock().expect("hitl queue poisoned");
        guard
            .heap
            .iter()
            .filter(|item| item.injection.tenant_id.into_inner() == tenant_id)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn cfg(&self) -> &HitlQueueConfig {
        &self.queue.cfg
    }

    pub fn peek_clarify_topk(&self, tenant: TenantId, k: usize) -> Vec<ClarifyQueueEntry> {
        self.queue.topk_for_tenant(tenant.into_inner(), k)
    }

    pub fn authorize_abort(
        &self,
        tenant: TenantId,
        injection_id: Uuid,
        authorized_by: impl Into<String>,
        reason: impl Into<String>,
    ) -> Option<ClarifyAbortRecord> {
        if let Some(injection) = self.queue.remove(injection_id) {
            if injection.tenant_id != tenant {
                // keep original ordering by re-enqueue
                self.queue.enqueue(injection);
                return None;
            }
            let record = ClarifyAbortRecord {
                injection_id,
                tenant_id: tenant,
                authorized_by: authorized_by.into(),
                reason: reason.into(),
                occurred_at: OffsetDateTime::now_utc(),
            };
            self.abort_records
                .lock()
                .expect("hitl abort records poisoned")
                .push(record.clone());
            Some(record)
        } else {
            None
        }
    }

    pub fn resolve_injection(&self, injection_id: Uuid) -> Option<HitlInjection> {
        self.queue.remove(injection_id)
    }

    pub fn abort_history(&self) -> Vec<ClarifyAbortRecord> {
        self.abort_records
            .lock()
            .expect("hitl abort records poisoned")
            .clone()
    }
}

/// 延迟注入队列 - 管理条件化的HITL注入
#[derive(Clone)]
pub struct DeferredInjectionQueue {
    state: Arc<Mutex<DeferredQueueState>>,
}

#[derive(Default)]
struct DeferredQueueState {
    /// 所有待触发的延迟注入
    pending: VecDeque<DeferredInjection>,
    /// 已触发并移入HITL队列的注入ID
    triggered: HashSet<Uuid>,
}

impl DeferredInjectionQueue {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DeferredQueueState::default())),
        }
    }

    /// 添加延迟注入到队列
    pub fn enqueue(&self, deferred: DeferredInjection) {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        guard.pending.push_back(deferred);
        tracing::debug!(
            "Deferred injection enqueued: {} items in queue",
            guard.pending.len()
        );
    }

    /// 检查并清理过期的延迟注入
    pub fn expire_old(&self, now: OffsetDateTime) -> Vec<DeferredInjection> {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        let mut expired = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(deferred) = guard.pending.pop_front() {
            if deferred.is_expired(now) {
                tracing::info!(
                    "Deferred injection expired: deferred_id={}, trigger={:?}",
                    deferred.deferred_id,
                    deferred.trigger_condition
                );
                expired.push(deferred);
            } else {
                retained.push_back(deferred);
            }
        }

        guard.pending = retained;
        expired
    }

    /// 检查时间触发条件
    pub fn check_time_triggers(&self, now: OffsetDateTime) -> Vec<HitlInjection> {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        let mut triggered = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(deferred) = guard.pending.pop_front() {
            let should_trigger = matches!(deferred.trigger_condition, TriggerCondition::TimeElapsed)
                && now >= deferred.expires_at;

            if should_trigger {
                tracing::info!(
                    "Time-based deferred injection triggered: deferred_id={}",
                    deferred.deferred_id
                );
                guard.triggered.insert(deferred.deferred_id);
                triggered.push(deferred.injection);
            } else {
                retained.push_back(deferred);
            }
        }

        guard.pending = retained;
        triggered
    }

    /// 检查预算超出触发条件
    pub fn check_budget_triggers(&self, usage_ratio: f32) -> Vec<HitlInjection> {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        let mut triggered = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(deferred) = guard.pending.pop_front() {
            let should_trigger = match &deferred.trigger_condition {
                TriggerCondition::BudgetExceeded { threshold } => {
                    usage_ratio >= (*threshold as f32 / 100.0)
                }
                _ => false,
            };

            if should_trigger {
                tracing::info!(
                    "Budget-based deferred injection triggered: deferred_id={}, usage={}%",
                    deferred.deferred_id,
                    usage_ratio * 100.0
                );
                guard.triggered.insert(deferred.deferred_id);
                triggered.push(deferred.injection);
            } else {
                retained.push_back(deferred);
            }
        }

        guard.pending = retained;
        triggered
    }

    /// 检查路由变更触发条件
    pub fn check_route_change_triggers(&self) -> Vec<HitlInjection> {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        let mut triggered = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(deferred) = guard.pending.pop_front() {
            let should_trigger = matches!(deferred.trigger_condition, TriggerCondition::RouteChanged);

            if should_trigger {
                tracing::info!(
                    "Route-change deferred injection triggered: deferred_id={}",
                    deferred.deferred_id
                );
                guard.triggered.insert(deferred.deferred_id);
                triggered.push(deferred.injection);
            } else {
                retained.push_back(deferred);
            }
        }

        guard.pending = retained;
        triggered
    }

    /// 检查周期完成触发条件
    pub fn check_cycle_completed_triggers(&self, cycle_id: AwarenessCycleId) -> Vec<HitlInjection> {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        let mut triggered = Vec::new();
        let mut retained = VecDeque::new();

        while let Some(deferred) = guard.pending.pop_front() {
            let should_trigger = match &deferred.trigger_condition {
                TriggerCondition::CycleCompleted { cycle_id: target_id } => {
                    *target_id == cycle_id
                }
                TriggerCondition::AnySyncPointCompleted => true,
                _ => false,
            };

            if should_trigger {
                tracing::info!(
                    "Cycle-completion deferred injection triggered: deferred_id={}, cycle_id={}",
                    deferred.deferred_id,
                    cycle_id.as_u64()
                );
                guard.triggered.insert(deferred.deferred_id);
                triggered.push(deferred.injection);
            } else {
                retained.push_back(deferred);
            }
        }

        guard.pending = retained;
        triggered
    }

    /// 获取队列中待处理的延迟注入数量
    pub fn len(&self) -> usize {
        self.state.lock().expect("deferred queue poisoned").pending.len()
    }

    /// 检查队列是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取已触发的注入数量
    pub fn triggered_count(&self) -> usize {
        self.state.lock().expect("deferred queue poisoned").triggered.len()
    }

    /// 清空队列（仅用于测试）
    pub fn clear(&self) {
        let mut guard = self.state.lock().expect("deferred queue poisoned");
        guard.pending.clear();
        guard.triggered.clear();
    }
}

impl Default for DeferredInjectionQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use soulseed_agi_core_models::TenantId;
    use time::Duration;

    #[test]
    fn priority_role_and_recency_are_respected() {
        let queue = HitlQueue::new(HitlQueueConfig::default());
        let tenant = TenantId::new(1);
        let base = OffsetDateTime::now_utc();

        queue.enqueue(
            HitlInjection::new(
                tenant,
                HitlPriority::P2Medium,
                "participant",
                json!({"id": 1}),
            )
            .with_submitted_at(base - Duration::seconds(30)),
        );
        queue.enqueue(
            HitlInjection::new(tenant, HitlPriority::P1High, "guest", json!({"id": 2}))
                .with_submitted_at(base - Duration::seconds(20)),
        );
        queue.enqueue(
            HitlInjection::new(tenant, HitlPriority::P1High, "system", json!({"id": 3}))
                .with_submitted_at(base - Duration::seconds(10)),
        );
        queue.enqueue(
            HitlInjection::new(tenant, HitlPriority::P3Low, "participant", json!({"id": 4}))
                .with_submitted_at(base - Duration::seconds(5)),
        );

        let active = HashSet::new();
        let first = queue.pop_ready(&active).expect("first");
        assert_eq!(first.payload["id"], 3);
        let second = queue.pop_ready(&active).expect("second");
        assert_eq!(second.payload["id"], 2);
        let third = queue.pop_ready(&active).expect("third");
        assert_eq!(third.payload["id"], 1);
    }

    #[test]
    fn active_cycle_allows_only_critical() {
        let queue = HitlQueue::new(HitlQueueConfig::default());
        let tenant = TenantId::new(2);
        queue.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "participant",
            json!({"id": 5}),
        ));
        queue.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P0Critical,
            "system",
            json!({"id": 6}),
        ));

        let mut active = HashSet::new();
        active.insert(tenant.into_inner());

        let critical = queue.pop_ready(&active).expect("critical");
        assert_eq!(critical.payload["id"], 6);
        assert!(queue.pop_ready(&active).is_none());

        active.clear();
        let remaining = queue.pop_ready(&active).expect("remaining");
        assert_eq!(remaining.payload["id"], 5);
    }

    #[test]
    fn clarify_topk_and_abort_flow() {
        let service = HitlService::new(HitlQueueConfig::default());
        let tenant = TenantId::new(3);
        let other = TenantId::new(4);
        service.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "system",
            json!({"id": 1}),
        ));
        service.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P2Medium,
            "facilitator",
            json!({"id": 2}),
        ));
        service.enqueue(HitlInjection::new(
            other,
            HitlPriority::P0Critical,
            "owner",
            json!({"id": 3}),
        ));

        let topk = service.peek_clarify_topk(tenant, 2);
        assert_eq!(topk.len(), 2);
        assert!(topk[0].reason.contains("priority=P1High"));
        assert_eq!(topk[0].injection.payload["id"], 1);
        assert_eq!(topk[1].injection.payload["id"], 2);

        let injection_id = topk[0].injection.injection_id;
        let abort = service
            .authorize_abort(tenant, injection_id, "clarify_owner", "abort_for_revision")
            .expect("abort");
        assert_eq!(abort.injection_id, injection_id);
        assert_eq!(abort.authorized_by, "clarify_owner");

        let history = service.abort_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].reason, "abort_for_revision");

        let topk_after = service.peek_clarify_topk(tenant, 2);
        assert_eq!(topk_after.len(), 1);
        assert_eq!(topk_after[0].injection.payload["id"], 2);
    }

    #[test]
    fn clarify_lane_allows_high_priority_interrupt() {
        let service = HitlService::new(HitlQueueConfig::default());
        let tenant = TenantId::new(6);
        service.mark_cycle_active(tenant);

        service.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "system",
            json!({"id": 8}),
        ));
        service.enqueue(HitlInjection::new(
            tenant,
            HitlPriority::P2Medium,
            "participant",
            json!({"id": 9}),
        ));

        let next = service.next_ready();
        assert!(
            next.is_none(),
            "regular queue is blocked while cycle active"
        );

        let clarify_next = service.next_ready_for_clarify().expect("clarify next");
        assert_eq!(clarify_next.payload["id"], 8);
    }

    #[test]
    fn deferred_injection_time_trigger() {
        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(10);
        let now = OffsetDateTime::now_utc();

        // 创建一个立即过期的延迟注入
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "system",
            json!({"id": 100, "reason": "time_trigger"}),
        );
        let deferred = DeferredInjection::new(
            injection,
            TriggerCondition::TimeElapsed,
            -1, // 已过期
        );

        queue.enqueue(deferred);
        assert_eq!(queue.len(), 1);

        // 检查时间触发
        let triggered = queue.check_time_triggers(now);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].payload["id"], 100);
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.triggered_count(), 1);
    }

    #[test]
    fn deferred_injection_budget_trigger() {
        use soulseed_agi_core_models::AwarenessCycleId;

        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(11);

        // 创建一个预算触发条件（阈值80%）
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P0Critical,
            "system",
            json!({"id": 101, "reason": "budget_exceeded"}),
        );
        let deferred = DeferredInjection::new(
            injection,
            TriggerCondition::BudgetExceeded { threshold: 80 },
            300, // 5分钟TTL
        );

        queue.enqueue(deferred);

        // 预算使用率低于阈值，不应触发
        let triggered = queue.check_budget_triggers(0.7);
        assert_eq!(triggered.len(), 0);
        assert_eq!(queue.len(), 1);

        // 预算使用率达到阈值，应触发
        let triggered = queue.check_budget_triggers(0.85);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].payload["id"], 101);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn deferred_injection_cycle_completed_trigger() {
        use soulseed_agi_core_models::AwarenessCycleId;

        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(12);
        let target_cycle = AwarenessCycleId::from_raw_unchecked(123);

        // 创建周期完成触发条件
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P2Medium,
            "participant",
            json!({"id": 102, "reason": "cycle_completed"}),
        );
        let deferred = DeferredInjection::new(
            injection,
            TriggerCondition::CycleCompleted { cycle_id: target_cycle },
            600, // 10分钟TTL
        );

        queue.enqueue(deferred);

        // 不同的周期完成，不应触发
        let other_cycle = AwarenessCycleId::from_raw_unchecked(456);
        let triggered = queue.check_cycle_completed_triggers(other_cycle);
        assert_eq!(triggered.len(), 0);
        assert_eq!(queue.len(), 1);

        // 目标周期完成，应触发
        let triggered = queue.check_cycle_completed_triggers(target_cycle);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].payload["id"], 102);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn deferred_injection_any_syncpoint_trigger() {
        use soulseed_agi_core_models::AwarenessCycleId;

        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(13);

        // 创建"任意SyncPoint完成"触发条件
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P3Low,
            "guest",
            json!({"id": 103, "reason": "any_syncpoint"}),
        );
        let deferred = DeferredInjection::new(
            injection,
            TriggerCondition::AnySyncPointCompleted,
            900, // 15分钟TTL
        );

        queue.enqueue(deferred);

        // 任意周期完成都应触发
        let any_cycle = AwarenessCycleId::from_raw_unchecked(999);
        let triggered = queue.check_cycle_completed_triggers(any_cycle);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].payload["id"], 103);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn deferred_injection_expiration() {
        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(14);
        let now = OffsetDateTime::now_utc();

        // 创建已过期的延迟注入
        let injection1 = HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "system",
            json!({"id": 104}),
        );
        let deferred1 = DeferredInjection::new(
            injection1,
            TriggerCondition::RouteChanged,
            -10, // 已过期10秒
        );

        // 创建未过期的延迟注入
        let injection2 = HitlInjection::new(
            tenant,
            HitlPriority::P2Medium,
            "facilitator",
            json!({"id": 105}),
        );
        let deferred2 = DeferredInjection::new(
            injection2,
            TriggerCondition::RouteChanged,
            3600, // 1小时TTL
        );

        queue.enqueue(deferred1);
        queue.enqueue(deferred2);
        assert_eq!(queue.len(), 2);

        // 清理过期项
        let expired = queue.expire_old(now);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].injection.payload["id"], 104);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn deferred_injection_route_change_trigger() {
        let queue = DeferredInjectionQueue::new();
        let tenant = TenantId::new(15);

        // 创建路由变更触发条件
        let injection = HitlInjection::new(
            tenant,
            HitlPriority::P1High,
            "system",
            json!({"id": 106, "reason": "route_changed"}),
        );
        let deferred = DeferredInjection::new(
            injection,
            TriggerCondition::RouteChanged,
            300,
        );

        queue.enqueue(deferred);
        assert_eq!(queue.len(), 1);

        // 检查路由变更触发
        let triggered = queue.check_route_change_triggers();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].payload["id"], 106);
        assert_eq!(queue.len(), 0);
    }
}

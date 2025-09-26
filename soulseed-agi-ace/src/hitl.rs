use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::TenantId;
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

    pub fn abort_history(&self) -> Vec<ClarifyAbortRecord> {
        self.abort_records
            .lock()
            .expect("hitl abort records poisoned")
            .clone()
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
}

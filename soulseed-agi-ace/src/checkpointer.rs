use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{CheckpointState, CycleLane};
use time::OffsetDateTime;

/// 缓存条目，包含状态和元数据
#[derive(Clone)]
struct CacheEntry {
    state: CheckpointState,
    last_accessed: OffsetDateTime,
    access_count: u64,
}

/// 缓存失效策略
#[derive(Clone, Debug)]
pub struct CacheEvictionPolicy {
    /// 最大缓存条目数
    pub max_entries: usize,
    /// TTL（秒）
    pub ttl_seconds: i64,
    /// 是否启用LRU
    pub enable_lru: bool,
}

impl Default for CacheEvictionPolicy {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl_seconds: 3600, // 1小时
            enable_lru: true,
        }
    }
}

#[derive(Default)]
struct CheckpointerState {
    cache: HashMap<(u64, u64), CacheEntry>,
    /// LRU访问顺序队列
    lru_queue: VecDeque<(u64, u64)>,
}

#[derive(Clone)]
pub struct Checkpointer {
    inner: Arc<Mutex<CheckpointerState>>,
    policy: CacheEvictionPolicy,
}

impl Default for Checkpointer {
    fn default() -> Self {
        Self::new(CacheEvictionPolicy::default())
    }
}

impl Checkpointer {
    pub fn new(policy: CacheEvictionPolicy) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CheckpointerState::default())),
            policy,
        }
    }

    /// 驱逐过期和超出容量的缓存条目
    fn evict_if_needed(&self, guard: &mut CheckpointerState) {
        let now = OffsetDateTime::now_utc();

        // 1. 移除过期条目（基于TTL）
        let expired_keys: Vec<(u64, u64)> = guard
            .cache
            .iter()
            .filter(|(_, entry)| {
                let age = (now - entry.last_accessed).whole_seconds();
                age > self.policy.ttl_seconds
            })
            .map(|(k, _)| *k)
            .collect();

        for key in expired_keys {
            guard.cache.remove(&key);
            guard.lru_queue.retain(|k| k != &key);
            tracing::debug!("Evicted expired checkpoint: {:?}", key);
        }

        // 2. 如果超出容量，使用LRU驱逐
        if self.policy.enable_lru {
            while guard.cache.len() >= self.policy.max_entries {
                if let Some(lru_key) = guard.lru_queue.pop_front() {
                    guard.cache.remove(&lru_key);
                    tracing::debug!("Evicted LRU checkpoint: {:?}", lru_key);
                } else {
                    break;
                }
            }
        }
    }

    /// 更新LRU队列
    fn update_lru(&self, guard: &mut CheckpointerState, key: (u64, u64)) {
        if !self.policy.enable_lru {
            return;
        }

        // 从队列中移除旧位置
        guard.lru_queue.retain(|k| k != &key);
        // 添加到队列末尾（最近使用）
        guard.lru_queue.push_back(key);
    }

    pub fn record(&self, state: CheckpointState) {
        let key = (state.tenant_id.into_inner(), state.cycle_id.as_u64());
        let mut guard = self.inner.lock().unwrap();

        // 驱逐策略检查
        self.evict_if_needed(&mut guard);

        // 创建缓存条目
        let entry = CacheEntry {
            state,
            last_accessed: OffsetDateTime::now_utc(),
            access_count: 1,
        };

        guard.cache.insert(key, entry);
        self.update_lru(&mut guard, key);
    }

    pub fn fetch(&self, tenant: u64, cycle: u64) -> Option<CheckpointState> {
        let key = (tenant, cycle);
        let mut guard = self.inner.lock().unwrap();

        if let Some(entry) = guard.cache.get_mut(&key) {
            // 更新访问元数据
            entry.last_accessed = OffsetDateTime::now_utc();
            entry.access_count += 1;
            let result = entry.state.clone();

            // 更新LRU（必须在使用entry之后）
            self.update_lru(&mut guard, key);
            Some(result)
        } else {
            None
        }
    }

    pub fn ensure_lane_idle(
        &self,
        tenant: u64,
        lane: &CycleLane,
        window: time::Duration,
    ) -> Result<(), AceError> {
        let guard = self.inner.lock().unwrap();
        if let Some((_, entry)) = guard
            .cache
            .iter()
            .find(|((t, _), entry)| *t == tenant && &entry.state.lane == lane)
        {
            let elapsed = OffsetDateTime::now_utc() - entry.state.since;
            if elapsed < window {
                return Err(AceError::ScheduleConflict(format!(
                    "lane {:?} busy for tenant {}",
                    lane, tenant
                )));
            }
        }
        Ok(())
    }

    pub fn finish(&self, tenant: u64) {
        let mut guard = self.inner.lock().unwrap();
        guard.cache.retain(|(t, _), _| *t != tenant);
        guard.lru_queue.retain(|(t, _)| *t != tenant);
    }

    /// 手动失效特定条目
    pub fn invalidate(&self, tenant: u64, cycle: u64) {
        let mut guard = self.inner.lock().unwrap();
        let key = (tenant, cycle);
        guard.cache.remove(&key);
        guard.lru_queue.retain(|k| k != &key);
        tracing::debug!("Manually invalidated checkpoint: {:?}", key);
    }

    /// 获取缓存统计信息
    pub fn cache_stats(&self) -> CacheStats {
        let guard = self.inner.lock().unwrap();
        CacheStats {
            total_entries: guard.cache.len(),
            lru_queue_len: guard.lru_queue.len(),
            policy: self.policy.clone(),
        }
    }
}

/// 缓存统计信息
#[derive(Clone, Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub lru_queue_len: usize,
    pub policy: CacheEvictionPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::{TenantId, AwarenessCycleId};
    use crate::types::BudgetSnapshot;

    fn create_dummy_state(tenant: u64, cycle: u64, lane: CycleLane) -> CheckpointState {
        CheckpointState {
            tenant_id: TenantId::from_raw_unchecked(tenant),
            cycle_id: AwarenessCycleId::from_raw_unchecked(cycle),
            lane,
            budget: BudgetSnapshot {
                tokens_allowed: 1000,
                tokens_spent: 100,
                walltime_ms_allowed: 10000,
                walltime_ms_used: 1000,
                external_cost_allowed: 100.0,
                external_cost_spent: 10.0,
            },
            since: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn test_cache_basic_record_and_fetch() {
        let checkpointer = Checkpointer::default();
        let state = create_dummy_state(1, 100, CycleLane::Clarify);

        checkpointer.record(state.clone());
        let fetched = checkpointer.fetch(1, 100);

        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().cycle_id.as_u64(), 100);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let policy = CacheEvictionPolicy {
            max_entries: 3,
            ttl_seconds: 3600,
            enable_lru: true,
        };
        let checkpointer = Checkpointer::new(policy);

        // 添加4个条目，超过max_entries
        checkpointer.record(create_dummy_state(1, 100, CycleLane::Clarify));
        checkpointer.record(create_dummy_state(1, 101, CycleLane::Tool));
        checkpointer.record(create_dummy_state(1, 102, CycleLane::SelfReason));
        checkpointer.record(create_dummy_state(1, 103, CycleLane::Collab));

        // 第一个条目应该被LRU驱逐
        assert!(checkpointer.fetch(1, 100).is_none(), "LRU should evict oldest entry");
        assert!(checkpointer.fetch(1, 101).is_some());
        assert!(checkpointer.fetch(1, 102).is_some());
        assert!(checkpointer.fetch(1, 103).is_some());

        let stats = checkpointer.cache_stats();
        assert_eq!(stats.total_entries, 3, "Should maintain max_entries limit");
    }

    #[test]
    fn test_cache_manual_invalidation() {
        let checkpointer = Checkpointer::default();
        let state = create_dummy_state(1, 100, CycleLane::Clarify);

        checkpointer.record(state);
        assert!(checkpointer.fetch(1, 100).is_some());

        checkpointer.invalidate(1, 100);
        assert!(checkpointer.fetch(1, 100).is_none(), "Should be invalidated");
    }

    #[test]
    fn test_cache_finish_tenant() {
        let checkpointer = Checkpointer::default();

        checkpointer.record(create_dummy_state(1, 100, CycleLane::Clarify));
        checkpointer.record(create_dummy_state(1, 101, CycleLane::Tool));
        checkpointer.record(create_dummy_state(2, 200, CycleLane::SelfReason));

        checkpointer.finish(1);

        assert!(checkpointer.fetch(1, 100).is_none());
        assert!(checkpointer.fetch(1, 101).is_none());
        assert!(checkpointer.fetch(2, 200).is_some(), "Other tenant should remain");
    }

    #[test]
    fn test_cache_stats() {
        let policy = CacheEvictionPolicy {
            max_entries: 10,
            ttl_seconds: 3600,
            enable_lru: true,
        };
        let checkpointer = Checkpointer::new(policy.clone());

        checkpointer.record(create_dummy_state(1, 100, CycleLane::Clarify));
        checkpointer.record(create_dummy_state(1, 101, CycleLane::Tool));

        let stats = checkpointer.cache_stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.lru_queue_len, 2);
        assert_eq!(stats.policy.max_entries, 10);
    }
}

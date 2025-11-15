use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use soulseed_agi_core_models::AwarenessCycleId;
use time::OffsetDateTime;

use crate::errors::AceError;
use crate::types::CycleEmission;

/// Barrier状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarrierState {
    /// 等待子周期完成
    Waiting,
    /// 所有子周期已就绪
    Ready,
    /// Barrier已完成（结果已合并）
    Completed,
}

/// Barrier群组，用于管理父周期的多个子周期
#[derive(Clone, Debug)]
pub struct BarrierGroup {
    /// 父周期ID
    pub parent_id: AwarenessCycleId,
    /// 协作范围ID
    pub scope_id: String,
    /// 期待的子周期数量
    pub expected_children: usize,
    /// 已注册的子周期ID集合
    pub registered_children: HashSet<AwarenessCycleId>,
    /// 已完成的子周期结果
    pub completed_children: HashMap<AwarenessCycleId, CycleEmission>,
    /// Barrier状态
    pub state: BarrierState,
    /// 创建时间
    pub created_at: OffsetDateTime,
    /// 最后更新时间
    pub updated_at: OffsetDateTime,
}

impl BarrierGroup {
    /// 创建新的Barrier群组
    pub fn new(
        parent_id: AwarenessCycleId,
        scope_id: String,
        expected_children: usize,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            parent_id,
            scope_id,
            expected_children,
            registered_children: HashSet::new(),
            completed_children: HashMap::new(),
            state: BarrierState::Waiting,
            created_at: now,
            updated_at: now,
        }
    }

    /// 注册子周期
    pub fn register_child(&mut self, child_id: AwarenessCycleId) -> Result<(), AceError> {
        if self.registered_children.len() >= self.expected_children {
            return Err(AceError::ScheduleConflict(format!(
                "Barrier group already has {} children (max: {})",
                self.registered_children.len(),
                self.expected_children
            )));
        }

        if self.registered_children.contains(&child_id) {
            return Err(AceError::ScheduleConflict(format!(
                "Child cycle {} already registered",
                child_id.as_u64()
            )));
        }

        self.registered_children.insert(child_id);
        self.updated_at = OffsetDateTime::now_utc();
        Ok(())
    }

    /// 记录子周期完成
    pub fn complete_child(
        &mut self,
        child_id: AwarenessCycleId,
        emission: CycleEmission,
    ) -> Result<(), AceError> {
        if !self.registered_children.contains(&child_id) {
            return Err(AceError::ScheduleConflict(format!(
                "Child cycle {} not registered in barrier group",
                child_id.as_u64()
            )));
        }

        if self.completed_children.contains_key(&child_id) {
            return Err(AceError::ScheduleConflict(format!(
                "Child cycle {} already completed",
                child_id.as_u64()
            )));
        }

        self.completed_children.insert(child_id, emission);
        self.updated_at = OffsetDateTime::now_utc();

        // 检查是否所有子周期都已完成
        if self.is_ready() {
            self.state = BarrierState::Ready;
            tracing::info!(
                "Barrier group ready: parent={}, scope={}, children={}/{}",
                self.parent_id.as_u64(),
                self.scope_id,
                self.completed_children.len(),
                self.expected_children
            );
        }

        Ok(())
    }

    /// 检查Barrier是否就绪（所有子周期都已完成）
    pub fn is_ready(&self) -> bool {
        self.completed_children.len() == self.expected_children
            && self.completed_children.len() == self.registered_children.len()
    }

    /// 获取所有已完成的子周期发射
    pub fn get_completed_emissions(&self) -> Vec<&CycleEmission> {
        self.completed_children.values().collect()
    }

    /// 标记Barrier已完成
    pub fn mark_completed(&mut self) {
        self.state = BarrierState::Completed;
        self.updated_at = OffsetDateTime::now_utc();
    }
}

/// Barrier管理器，管理所有活跃的Barrier群组
#[derive(Clone, Default)]
pub struct BarrierManager {
    /// 按父周期ID索引的Barrier群组
    groups: Arc<Mutex<HashMap<AwarenessCycleId, BarrierGroup>>>,
}

impl BarrierManager {
    pub fn new() -> Self {
        Self {
            groups: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建新的Barrier群组
    pub fn create_group(
        &self,
        parent_id: AwarenessCycleId,
        scope_id: String,
        expected_children: usize,
    ) -> Result<(), AceError> {
        let mut groups = self.groups.lock().unwrap();

        if groups.contains_key(&parent_id) {
            return Err(AceError::ScheduleConflict(format!(
                "Barrier group already exists for parent {}",
                parent_id.as_u64()
            )));
        }

        let group = BarrierGroup::new(parent_id, scope_id, expected_children);
        groups.insert(parent_id, group);

        tracing::debug!(
            "Created barrier group: parent={}, expected_children={}",
            parent_id.as_u64(),
            expected_children
        );

        Ok(())
    }

    /// 注册子周期到群组
    pub fn register_child(
        &self,
        parent_id: AwarenessCycleId,
        child_id: AwarenessCycleId,
    ) -> Result<(), AceError> {
        let mut groups = self.groups.lock().unwrap();

        let group = groups.get_mut(&parent_id).ok_or_else(|| {
            AceError::ScheduleConflict(format!(
                "Barrier group not found for parent {}",
                parent_id.as_u64()
            ))
        })?;

        group.register_child(child_id)
    }

    /// 完成子周期
    pub fn complete_child(
        &self,
        parent_id: AwarenessCycleId,
        child_id: AwarenessCycleId,
        emission: CycleEmission,
    ) -> Result<bool, AceError> {
        let mut groups = self.groups.lock().unwrap();

        let group = groups.get_mut(&parent_id).ok_or_else(|| {
            AceError::ScheduleConflict(format!(
                "Barrier group not found for parent {}",
                parent_id.as_u64()
            ))
        })?;

        group.complete_child(child_id, emission)?;
        Ok(group.is_ready())
    }

    /// 获取群组（用于Barrier聚合）
    pub fn get_group(&self, parent_id: AwarenessCycleId) -> Option<BarrierGroup> {
        let groups = self.groups.lock().unwrap();
        groups.get(&parent_id).cloned()
    }

    /// 标记群组已完成并移除
    pub fn finish_group(&self, parent_id: AwarenessCycleId) -> Result<(), AceError> {
        let mut groups = self.groups.lock().unwrap();
        groups.remove(&parent_id);

        tracing::debug!("Finished barrier group: parent={}", parent_id.as_u64());
        Ok(())
    }

    /// 获取活跃群组数量（用于监控）
    pub fn active_groups_count(&self) -> usize {
        let groups = self.groups.lock().unwrap();
        groups.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 简化的测试辅助函数 - 创建最小化的CycleEmission用于测试
    fn create_minimal_emission(cycle_id: u64) -> CycleEmission {
        // 使用 serde_json 来反序列化一个最小的JSON，避免手动构造复杂的类型
        let minimal_json = serde_json::json!({
            "cycle_id": cycle_id,
            "lane": "tool",
            "final_event": {
                "base": {
                    "tenant_id": 1,
                    "session_id": 1,
                    "participants": [],
                    "head": {
                        "envelope_id": "00000000-0000-0000-0000-000000000000",
                        "config_snapshot_hash": "test",
                        "config_snapshot_version": 1,
                    },
                    "event_id": 100,
                    "ac_id": cycle_id,
                    "timestamp_ms": 0,
                    "delegation_trace": [],
                    "trace_id": "trace-1",
                },
                "payload": {
                    "assistant": {
                        "content": [{
                            "text": "test"
                        }]
                    }
                }
            },
            "awareness_events": [],
            "budget": {
                "tokens_allowed": 1000,
                "tokens_spent": 100,
                "walltime_ms_allowed": 10000,
                "walltime_ms_used": 1000,
                "external_cost_allowed": 100.0,
                "external_cost_spent": 10.0,
            },
            "anchor": {
                "tenant_id": 1,
                "session_id": 1,
                "envelope_id": "00000000-0000-0000-0000-000000000000",
                "config_snapshot_hash": "test",
                "config_snapshot_version": 1,
                "provenance": "test",
                "schema_v": 1,
            },
            "router_decision": {
                "plan": {
                    "fork": "ToolPath",
                    "anchor": {
                        "tenant_id": 1,
                        "session_id": 1,
                        "envelope_id": "00000000-0000-0000-0000-000000000000",
                        "config_snapshot_hash": "test",
                        "config_snapshot_version": 1,
                        "provenance": "test",
                        "schema_v": 1,
                    }
                },
                "decision_path": {
                    "steps": []
                },
                "rejected": [],
                "context_digest": "",
                "issued_at": "2024-01-01T00:00:00Z"
            },
            "status": "Completed",
            "parent_cycle_id": 1,
            "collab_scope_id": "scope-1",
        });

        serde_json::from_value(minimal_json).expect("Failed to create minimal emission")
    }

    #[test]
    #[ignore] // TODO: 需要在集成测试中测试完整的emission结构
    fn test_barrier_group_basic() {
        let parent_id = AwarenessCycleId::from_raw_unchecked(1);
        let mut group = BarrierGroup::new(parent_id, "scope-1".to_string(), 2);

        assert_eq!(group.state, BarrierState::Waiting);
        assert!(!group.is_ready());

        let child1 = AwarenessCycleId::from_raw_unchecked(10);
        let child2 = AwarenessCycleId::from_raw_unchecked(20);

        group.register_child(child1).unwrap();
        group.register_child(child2).unwrap();

        assert!(!group.is_ready());

        group.complete_child(child1, create_minimal_emission(10)).unwrap();
        assert!(!group.is_ready());
        assert_eq!(group.state, BarrierState::Waiting);

        group.complete_child(child2, create_minimal_emission(20)).unwrap();
        assert!(group.is_ready());
        assert_eq!(group.state, BarrierState::Ready);
    }

    #[test]
    fn test_barrier_group_duplicate_registration() {
        let parent_id = AwarenessCycleId::from_raw_unchecked(1);
        let mut group = BarrierGroup::new(parent_id, "scope-1".to_string(), 2);

        let child1 = AwarenessCycleId::from_raw_unchecked(10);

        group.register_child(child1).unwrap();
        let result = group.register_child(child1);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // TODO: 需要在集成测试中测试完整的emission结构
    fn test_barrier_manager_workflow() {
        let manager = BarrierManager::new();
        let parent_id = AwarenessCycleId::from_raw_unchecked(1);

        manager
            .create_group(parent_id, "scope-1".to_string(), 2)
            .unwrap();

        let child1 = AwarenessCycleId::from_raw_unchecked(10);
        let child2 = AwarenessCycleId::from_raw_unchecked(20);

        manager.register_child(parent_id, child1).unwrap();
        manager.register_child(parent_id, child2).unwrap();

        let is_ready = manager
            .complete_child(parent_id, child1, create_minimal_emission(10))
            .unwrap();
        assert!(!is_ready);

        let is_ready = manager
            .complete_child(parent_id, child2, create_minimal_emission(20))
            .unwrap();
        assert!(is_ready);

        let group = manager.get_group(parent_id).unwrap();
        assert_eq!(group.state, BarrierState::Ready);
        assert_eq!(group.completed_children.len(), 2);

        manager.finish_group(parent_id).unwrap();
        assert!(manager.get_group(parent_id).is_none());
    }

    #[test]
    fn test_barrier_manager_active_count() {
        let manager = BarrierManager::new();

        assert_eq!(manager.active_groups_count(), 0);

        manager
            .create_group(
                AwarenessCycleId::from_raw_unchecked(1),
                "scope-1".to_string(),
                2,
            )
            .unwrap();

        assert_eq!(manager.active_groups_count(), 1);

        manager
            .create_group(
                AwarenessCycleId::from_raw_unchecked(2),
                "scope-2".to_string(),
                3,
            )
            .unwrap();

        assert_eq!(manager.active_groups_count(), 2);

        manager
            .finish_group(AwarenessCycleId::from_raw_unchecked(1))
            .unwrap();

        assert_eq!(manager.active_groups_count(), 1);
    }
}

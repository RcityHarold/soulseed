use crate::errors::AceError;
use crate::types::{CycleEmission, OutboxMessage};
use serde_json::Value;
use soulseed_agi_core_models::{AwarenessCycleId, TenantId, awareness::AwarenessEvent};

pub trait AcePersistence: Send + Sync {
    fn persist_cycle(&self, emission: &CycleEmission) -> Result<(), AceError>;

    /// 保存周期快照（用于 API 查询）
    fn persist_cycle_snapshot(
        &self,
        tenant_id: TenantId,
        cycle_id: AwarenessCycleId,
        snapshot: &Value,
    ) -> Result<(), AceError>;

    /// 加载周期快照
    fn load_cycle_snapshot(
        &self,
        tenant_id: TenantId,
        cycle_id: AwarenessCycleId,
    ) -> Result<Option<Value>, AceError>;

    fn list_awareness_events(
        &self,
        tenant_id: TenantId,
        limit: usize,
    ) -> Result<Vec<AwarenessEvent>, AceError>;

    /// Checkpoint + Outbox 同事务保存
    ///
    /// 确保checkpoint和outbox消息在同一事务中持久化，
    /// 保证一致性：要么都成功，要么都失败。
    fn transactional_checkpoint_and_outbox(
        &self,
        tenant_id: TenantId,
        cycle_id: AwarenessCycleId,
        snapshot: &Value,
        outbox_messages: &[OutboxMessage],
    ) -> Result<(), AceError>;

    /// 查询pending状态的outbox消息
    fn list_pending_outbox(
        &self,
        tenant_id: TenantId,
        limit: usize,
    ) -> Result<Vec<OutboxMessage>, AceError>;

    /// 标记outbox消息为已发送
    fn mark_outbox_sent(
        &self,
        tenant_id: TenantId,
        event_ids: &[soulseed_agi_core_models::EventId],
    ) -> Result<(), AceError>;
}

#[cfg(feature = "persistence-surreal")]
pub mod surreal;

#[cfg(feature = "persistence-surreal")]
pub use surreal::{SurrealPersistence, SurrealPersistenceConfig};

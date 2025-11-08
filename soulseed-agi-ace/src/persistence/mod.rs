use crate::errors::AceError;
use crate::types::CycleEmission;
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
}

#[cfg(feature = "persistence-surreal")]
pub mod surreal;

#[cfg(feature = "persistence-surreal")]
pub use surreal::{SurrealPersistence, SurrealPersistenceConfig};

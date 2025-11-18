#![cfg(feature = "persistence-surreal")]

use std::future::Future;
use std::sync::Arc;

use soulbase_storage::model::{Entity, make_record_id, QueryParams};
use soulbase_storage::prelude::Repository;
use soulbase_storage::surreal::config::SurrealConfig;
use soulbase_storage::surreal::datastore::SurrealDatastore;
use soulbase_types::prelude::TenantId as SbTenantId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::AceError;
use crate::persistence::AcePersistence;
use crate::types::CycleEmission;
use soulseed_agi_core_models::awareness::AwarenessEvent;
use time::OffsetDateTime;

// Schema 定义已移至 migrations/surreal/001_soulseed_init.sql
// 请手动运行 SQL 文件来初始化数据库 schema

fn tenant_to_sb(tenant: soulseed_agi_core_models::TenantId) -> SbTenantId {
    SbTenantId(tenant.as_u64().to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DialogueEventDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    #[serde(serialize_with = "serialize_tenant_id", deserialize_with = "deserialize_tenant_id")]
    pub tenant: SbTenantId,
    pub event_id: String,
    pub cycle_id: String,
    pub occurred_at: i64,
    pub lane: String,
    pub payload: Value,
    pub metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain_fingerprint: Option<String>,
    pub created_at: i64,
}

// Serde helpers for TenantId
fn serialize_tenant_id<S>(tenant: &SbTenantId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&tenant.0)
}

fn deserialize_tenant_id<'de, D>(deserializer: D) -> Result<SbTenantId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SbTenantId(s))
}

impl Entity for DialogueEventDoc {
    const TABLE: &'static str = "ace_dialogue_event";

    fn id(&self) -> &str {
        &self.id
    }

    fn tenant(&self) -> &SbTenantId {
        &self.tenant
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AwarenessEventDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    #[serde(serialize_with = "serialize_tenant_id", deserialize_with = "deserialize_tenant_id")]
    pub tenant: SbTenantId,
    pub event_id: String,
    pub cycle_id: String,
    pub event_type: String,
    pub occurred_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_cycle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collab_scope_id: Option<String>,
    // 重命名为 event_data 避免与 AwarenessEvent.payload 字段冲突
    pub event_data: Value,
    pub created_at: i64,
}

impl Entity for AwarenessEventDoc {
    const TABLE: &'static str = "ace_awareness_event";

    fn id(&self) -> &str {
        &self.id
    }

    fn tenant(&self) -> &SbTenantId {
        &self.tenant
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CycleSnapshotDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    #[serde(serialize_with = "serialize_tenant_id", deserialize_with = "deserialize_tenant_id")]
    pub tenant: SbTenantId,
    pub cycle_id: String,
    // 改名为 snapshot_data 避免与 CycleSnapshot 中可能存在的 snapshot 字段冲突
    pub snapshot_data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Entity for CycleSnapshotDoc {
    const TABLE: &'static str = "ace_cycle_snapshot";

    fn id(&self) -> &str {
        &self.id
    }

    fn tenant(&self) -> &SbTenantId {
        &self.tenant
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OutboxMessageDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    #[serde(serialize_with = "serialize_tenant_id", deserialize_with = "deserialize_tenant_id")]
    pub tenant: SbTenantId,
    pub event_id: String,
    pub cycle_id: String,
    pub payload: Value,
    pub status: String, // "pending" or "sent"
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<i64>,
}

impl Entity for OutboxMessageDoc {
    const TABLE: &'static str = "ace_outbox";

    fn id(&self) -> &str {
        &self.id
    }

    fn tenant(&self) -> &SbTenantId {
        &self.tenant
    }
}

pub struct SurrealPersistence {
    handle: tokio::runtime::Handle,
    _owned_runtime: Option<std::mem::ManuallyDrop<tokio::runtime::Runtime>>,
    dialogue_repo: Arc<soulbase_storage::surreal::repo::SurrealRepository<DialogueEventDoc>>,
    awareness_repo: Arc<soulbase_storage::surreal::repo::SurrealRepository<AwarenessEventDoc>>,
    snapshot_repo: Arc<soulbase_storage::surreal::repo::SurrealRepository<CycleSnapshotDoc>>,
    outbox_repo: Arc<soulbase_storage::surreal::repo::SurrealRepository<OutboxMessageDoc>>,
    datastore: SurrealDatastore,
}

impl Drop for SurrealPersistence {
    fn drop(&mut self) {
        // 安全地处理 owned runtime：完全泄漏它以避免在异步上下文中 panic
        // 这是可接受的，因为 runtime 通常在程序整个生命周期中存在
        if let Some(runtime) = self._owned_runtime.take() {
            let runtime = unsafe { std::mem::ManuallyDrop::into_inner(runtime) };
            std::mem::forget(runtime); // 泄漏 runtime，避免 drop panic
        }
    }
}

#[derive(Clone, Debug)]
pub struct SurrealPersistenceConfig {
    pub datastore: SurrealConfig,
}

impl Default for SurrealPersistenceConfig {
    fn default() -> Self {
        Self {
            datastore: SurrealConfig::default(),
        }
    }
}

impl SurrealPersistence {
    async fn init_components(
        datastore_cfg: SurrealConfig,
    ) -> Result<
        (
            SurrealDatastore,
            Arc<soulbase_storage::surreal::repo::SurrealRepository<DialogueEventDoc>>,
            Arc<soulbase_storage::surreal::repo::SurrealRepository<AwarenessEventDoc>>,
            Arc<soulbase_storage::surreal::repo::SurrealRepository<CycleSnapshotDoc>>,
            Arc<soulbase_storage::surreal::repo::SurrealRepository<OutboxMessageDoc>>,
        ),
        AceError,
    > {
        let datastore = SurrealDatastore::connect(datastore_cfg.clone())
            .await
            .map_err(|err| AceError::Persistence(format!("surreal connect failed: {err}")))?;
        let dialogue_repo = Arc::new(soulbase_storage::surreal::repo::SurrealRepository::<
            DialogueEventDoc,
        >::new(&datastore));
        let awareness_repo = Arc::new(soulbase_storage::surreal::repo::SurrealRepository::<
            AwarenessEventDoc,
        >::new(&datastore));
        let snapshot_repo = Arc::new(soulbase_storage::surreal::repo::SurrealRepository::<
            CycleSnapshotDoc,
        >::new(&datastore));
        let outbox_repo = Arc::new(soulbase_storage::surreal::repo::SurrealRepository::<
            OutboxMessageDoc,
        >::new(&datastore));
        Ok((datastore, dialogue_repo, awareness_repo, snapshot_repo, outbox_repo))
    }

    pub fn new_with_handle(
        handle: tokio::runtime::Handle,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo, outbox_repo) =
            handle.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            _owned_runtime: None,
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
            outbox_repo,
            datastore,
        })
    }

    /// 异步初始化（用于已在 tokio runtime 中的场景）
    pub async fn new_async(config: SurrealPersistenceConfig) -> Result<Self, AceError> {
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo, outbox_repo) =
            Self::init_components(config.datastore).await?;

        let handle = tokio::runtime::Handle::current();

        Ok(Self {
            handle,
            _owned_runtime: None,
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
            outbox_repo,
            datastore,
        })
    }

    pub fn new_with_owned_runtime(
        runtime: tokio::runtime::Runtime,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let handle = runtime.handle().clone();
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo, outbox_repo) =
            runtime.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            _owned_runtime: Some(std::mem::ManuallyDrop::new(runtime)),
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
            outbox_repo,
            datastore,
        })
    }

    // ensure_schema() 方法已移除
    // Schema 通过手动运行 migrations/surreal/001_soulseed_init.sql 来初始化

    fn block_on<F, T>(&self, fut: F) -> Result<T, AceError>
    where
        F: Future<Output = Result<T, AceError>>,
    {
        if tokio::runtime::Handle::try_current().is_ok() {
            let handle = self.handle.clone();
            tokio::task::block_in_place(|| handle.block_on(fut))
        } else {
            self.handle.block_on(fut)
        }
    }

    fn build_dialogue_doc(emission: &CycleEmission) -> Result<DialogueEventDoc, AceError> {
        let tenant = tenant_to_sb(emission.anchor.tenant_id);
        let cycle_id = emission.cycle_id.as_u64().to_string();
        let event_id = emission.final_event.base.event_id.as_u64().to_string();
        let lane = format!("{:?}", emission.lane);
        let payload = serde_json::to_value(&emission.final_event).map_err(|err| {
            AceError::Persistence(format!("serialize dialogue event failed: {err}"))
        })?;
        let metadata = if emission.final_event.metadata.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            emission.final_event.metadata.clone()
        };
        Ok(DialogueEventDoc {
            id: make_record_id(
                DialogueEventDoc::TABLE,
                &tenant,
                &event_id,
            ),
            tenant,
            event_id,
            cycle_id,
            occurred_at: emission.final_event.base.timestamp_ms,
            lane,
            payload,
            metadata,
            manifest_digest: emission.manifest_digest.clone(),
            explain_fingerprint: emission.explain_fingerprint.clone(),
            created_at: current_millis(),
        })
    }

    fn build_awareness_doc(
        emission: &CycleEmission,
        event: &AwarenessEvent,
    ) -> Result<AwarenessEventDoc, AceError> {
        let tenant = tenant_to_sb(emission.anchor.tenant_id);
        let cycle_id = emission.cycle_id.as_u64().to_string();
        let event_id = event.event_id.as_u64().to_string();
        let event_type = serde_json::to_value(event.event_type)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or_else(|| format!("{:?}", event.event_type));

        // 直接序列化整个 AwarenessEvent
        let event_data = serde_json::to_value(event).map_err(|err| {
            AceError::Persistence(format!("serialize awareness event failed: {err}"))
        })?;

        Ok(AwarenessEventDoc {
            id: make_record_id(
                AwarenessEventDoc::TABLE,
                &tenant,
                &event_id,
            ),
            tenant,
            event_id,
            cycle_id,
            event_type,
            occurred_at: event.occurred_at_ms,
            parent_cycle_id: event.parent_cycle_id.map(|id| id.as_u64().to_string()),
            collab_scope_id: event.collab_scope_id.clone(),
            event_data,
            created_at: current_millis(),
        })
    }
}

fn current_millis() -> i64 {
    (OffsetDateTime::now_utc().unix_timestamp()) * 1000
}

impl AcePersistence for SurrealPersistence {
    fn persist_cycle(&self, emission: &CycleEmission) -> Result<(), AceError> {
        // Schema 已通过 migrations/surreal/001_soulseed_init.sql 初始化

        let dialogue = Self::build_dialogue_doc(emission)?;
        let awareness: Vec<AwarenessEventDoc> = emission
            .awareness_events
            .iter()
            .map(|evt| Self::build_awareness_doc(emission, evt))
            .collect::<Result<_, _>>()?;

        let sb_tenant = tenant_to_sb(emission.anchor.tenant_id);
        let dialogue_repo = self.dialogue_repo.clone();
        let awareness_repo = self.awareness_repo.clone();

        self.block_on(async move {
            let mut patch = serde_json::to_value(&dialogue).map_err(|err| {
                AceError::Persistence(format!("serialize dialogue patch failed: {err}"))
            })?;
            let _patch_json = serde_json::to_string(&patch).map_err(|err| {
                AceError::Persistence(format!("dialogue patch stringify failed: {err}"))
            })?;
            if let serde_json::Value::Object(ref mut map) = patch {
                map.remove("id");
                map.remove("tenant");  // tenant field cannot be updated
            }
            let patch_for_log = patch.clone();
            dialogue_repo
                .upsert_or_create(&sb_tenant, dialogue.event_id.as_str(), patch)
                .await
                .map_err(|err| {
                    let err_obj = err.into_inner();
                    let public = err_obj.to_public();
                    let audit = err_obj.to_audit();
                    let detail = audit.message_dev.as_ref().unwrap_or(&public.message).to_string();
                    tracing::error!(
                        code = %public.code,
                        message = %public.message,
                        message_dev = audit.message_dev.unwrap_or_default(),
                        correlation_id = audit.correlation_id.unwrap_or_default(),
                        meta = ?audit.meta,
                        tenant = %dialogue.tenant.0,
                        event_id = %dialogue.event_id,
                        patch = ?patch_for_log,
                        "persist dialogue event failed"
                    );
                    AceError::Persistence(format!("persist dialogue event failed: {}", detail))
                })?;

            for doc in awareness {
                // 添加调试日志：检查 event_data 是否为空
                tracing::debug!(
                    "Building awareness doc: event_id={}, event_type={}, event_data_is_null={}, event_data_size={}",
                    doc.event_id,
                    doc.event_type,
                    doc.event_data.is_null(),
                    serde_json::to_string(&doc.event_data).map(|s| s.len()).unwrap_or(0)
                );

                let mut patch = serde_json::to_value(&doc).map_err(|err| {
                    AceError::Persistence(format!("serialize awareness patch failed: {err}"))
                })?;

                // 添加调试日志：检查序列化后的 patch
                if let Some(event_data_field) = patch.get("event_data") {
                    tracing::debug!(
                        "Serialized patch for {}: event_data_is_null={}, event_data_is_object={}, event_data_keys={:?}",
                        doc.event_id,
                        event_data_field.is_null(),
                        event_data_field.is_object(),
                        event_data_field.as_object().map(|o| o.keys().collect::<Vec<_>>())
                    );
                } else {
                    tracing::warn!("Serialized patch for {} has no event_data field!", doc.event_id);
                }

                let patch_json = serde_json::to_string(&patch).map_err(|err| {
                    AceError::Persistence(format!("awareness patch stringify failed: {err}"))
                })?;

                tracing::debug!(
                    "Patch JSON for {}: length={}, preview={}",
                    doc.event_id,
                    patch_json.len(),
                    &patch_json[..patch_json.len().min(200)]
                );

                if let serde_json::Value::Object(ref mut map) = patch {
                    map.remove("id");
                    map.remove("tenant");  // tenant field cannot be updated
                }
                let patch_for_log = patch.clone();
                awareness_repo
                    .upsert_or_create(&sb_tenant, doc.event_id.as_str(), patch)
                    .await
                    .map_err(|err| {
                        let err_obj = err.into_inner();
                        let public = err_obj.to_public();
                        let audit = err_obj.to_audit();
                        let detail = audit.message_dev.as_ref().unwrap_or(&public.message).to_string();
                        tracing::error!(
                            code = %public.code,
                            message = %public.message,
                            message_dev = audit.message_dev.unwrap_or_default(),
                            correlation_id = audit.correlation_id.unwrap_or_default(),
                            meta = ?audit.meta,
                            tenant = %doc.tenant.0,
                            event_id = %doc.event_id,
                            patch = ?patch_for_log,
                            "persist awareness event failed"
                        );
                        AceError::Persistence(format!("persist awareness event failed: {}", detail))
                    })?;

                tracing::info!(
                    "Successfully persisted awareness event: tenant={}, event_id={}, event_type={}",
                    doc.tenant.0,
                    doc.event_id,
                    doc.event_type
                );
            }

            Ok::<_, AceError>(())
        })
    }

    fn list_awareness_events(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        limit: usize,
    ) -> Result<Vec<AwarenessEvent>, AceError> {
        let limit = limit.clamp(1, 500);
        let sb_tenant = tenant_to_sb(tenant_id);
        let repo = self.awareness_repo.clone();
        self.block_on(async move {
            let params = QueryParams {
                filter: serde_json::Value::Null,
                order_by: Some("occurred_at DESC".to_string()),
                limit: Some(limit as u32),
                cursor: None,
            };
            let page = repo
                .select(&sb_tenant, params)
                .await
                .map_err(|err| {
                    AceError::Persistence(format!("list awareness events failed: {err}"))
                })?;
            let mut events = Vec::with_capacity(page.items.len());
            for doc in page.items {
                match serde_json::from_value::<AwarenessEvent>(doc.event_data.clone()) {
                    Ok(event) => events.push(event),
                    Err(err) => {
                        tracing::warn!(
                            tenant = %doc.tenant.0,
                            event_id = %doc.event_id,
                            error = %err,
                            "skip awareness event row due to deserialize failure"
                        );
                        continue;
                    }
                }
            }
            Ok(events)
        })
    }

    fn persist_cycle_snapshot(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        snapshot: &Value,
    ) -> Result<(), AceError> {
        // Schema 已通过 migrations/surreal/001_soulseed_init.sql 初始化

        let sb_tenant = tenant_to_sb(tenant_id);
        let cycle_id_str = cycle_id.as_u64().to_string();
        let now = current_millis();

        let doc = CycleSnapshotDoc {
            id: make_record_id(
                CycleSnapshotDoc::TABLE,
                &sb_tenant,
                &cycle_id_str,
            ),
            tenant: sb_tenant.clone(),
            cycle_id: cycle_id_str.clone(),
            snapshot_data: snapshot.clone(),
            created_at: now,
            updated_at: now,
        };
        let snapshot_repo = self.snapshot_repo.clone();

        self.block_on(async move {
            let mut patch = serde_json::to_value(&doc).map_err(|err| {
                AceError::Persistence(format!("serialize snapshot patch failed: {err}"))
            })?;
            let _patch_json = serde_json::to_string(&patch).map_err(|err| {
                AceError::Persistence(format!("snapshot patch stringify failed: {err}"))
            })?;
            if let serde_json::Value::Object(ref mut map) = patch {
                map.remove("id");
                map.remove("tenant");  // tenant field cannot be updated
            }
            let patch_for_log = patch.clone();
            snapshot_repo
                .upsert_returning_none(&sb_tenant, &cycle_id_str, patch)
                .await
                .map_err(|err| {
                    let err_obj = err.into_inner();
                    let public = err_obj.to_public();
                    let audit = err_obj.to_audit();
                    let detail = audit.message_dev.as_ref().unwrap_or(&public.message).to_string();
                    tracing::error!(
                        code = %public.code,
                        message = %public.message,
                        message_dev = audit.message_dev.unwrap_or_default(),
                        correlation_id = audit.correlation_id.unwrap_or_default(),
                        meta = ?audit.meta,
                        tenant = %sb_tenant.0,
                        cycle_id = %cycle_id_str,
                        patch = ?patch_for_log,
                        "persist cycle snapshot failed"
                    );
                    AceError::Persistence(format!("persist cycle snapshot failed: {}", detail))
                })?;

            Ok::<_, AceError>(())
        })
    }

    fn load_cycle_snapshot(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        cycle_id: soulseed_agi_core_models::AwarenessCycleId,
    ) -> Result<Option<Value>, AceError> {
        // Schema 已通过 migrations/surreal/001_soulseed_init.sql 初始化

        let cycle_id_str = cycle_id.as_u64().to_string();
        let tenant = tenant_id.as_u64().to_string();
        let datastore = self.datastore.clone();

        tracing::debug!(
            "Loading cycle snapshot: tenant_id={}, cycle_id={}, cycle_id_str={}",
            tenant_id.as_u64(),
            cycle_id.as_u64(),
            cycle_id_str
        );

        self.block_on(async move {
            // 使用 WHERE 子句查询，避免记录 ID 格式问题
            let query = format!(
                "SELECT snapshot_data FROM {} WHERE cycle_id = '{}' AND tenant = '{}' LIMIT 1;",
                CycleSnapshotDoc::TABLE,
                cycle_id_str,
                tenant
            );

            tracing::debug!("Executing query: {}", query);

            use soulbase_storage::spi::datastore::Datastore;
            let session = datastore
                .session()
                .await
                .map_err(|err| {
                    tracing::error!(
                        error = %err,
                        tenant = %tenant,
                        cycle_id = %cycle_id_str,
                        "Failed to create session"
                    );
                    AceError::Persistence(format!("create session failed: {}", err))
                })?;

            let mut result = session
                .client()
                .query(&query)
                .await
                .map_err(|err| {
                    tracing::error!(
                        error = %err,
                        tenant = %tenant,
                        cycle_id = %cycle_id_str,
                        query = %query,
                        "SurrealDB query failed"
                    );
                    AceError::Persistence(format!("query cycle snapshot failed: {}", err))
                })?;

            // 获取查询结果
            let response: Option<Value> = result.take(0).map_err(|err| {
                tracing::error!(
                    error = %err,
                    tenant = %tenant,
                    cycle_id = %cycle_id_str,
                    "Failed to extract query result"
                );
                AceError::Persistence(format!("extract query result failed: {}", err))
            })?;

            tracing::debug!("Query response: {:?}", response);

            // 解析返回的结果
            // SurrealDB 的 result.take(0) 返回单个对象（第一行），而不是数组
            match response {
                Some(Value::Object(obj)) => {
                    // 直接从对象中提取 snapshot_data 字段
                    if let Some(snapshot_data) = obj.get("snapshot_data") {
                        tracing::debug!(
                            "Loaded snapshot_data: is_null={}, is_object={}, keys={:?}",
                            snapshot_data.is_null(),
                            snapshot_data.is_object(),
                            snapshot_data.as_object().map(|o| o.keys().collect::<Vec<_>>())
                        );
                        Ok(Some(snapshot_data.clone()))
                    } else {
                        tracing::warn!("snapshot_data field not found in result");
                        Ok(None)
                    }
                }
                Some(Value::Array(arr)) if !arr.is_empty() => {
                    // 兼容：如果返回的是数组，取第一个元素
                    if let Some(Value::Object(obj)) = arr.get(0) {
                        if let Some(snapshot_data) = obj.get("snapshot_data") {
                            tracing::debug!(
                                "Loaded snapshot_data from array: is_null={}, is_object={}, keys={:?}",
                                snapshot_data.is_null(),
                                snapshot_data.is_object(),
                                snapshot_data.as_object().map(|o| o.keys().collect::<Vec<_>>())
                            );
                            Ok(Some(snapshot_data.clone()))
                        } else {
                            tracing::warn!("snapshot_data field not found in array result");
                            Ok(None)
                        }
                    } else {
                        tracing::debug!("Empty result set in array");
                        Ok(None)
                    }
                }
                Some(Value::Array(_)) => {
                    tracing::debug!("Empty array, no snapshot found for cycle_id={}", cycle_id_str);
                    Ok(None)
                }
                Some(other) => {
                    tracing::warn!("Unexpected response format: {:?}", other);
                    Ok(None)
                }
                None => {
                    tracing::debug!("No snapshot found for cycle_id={}", cycle_id_str);
                    Ok(None)
                }
            }
        })
    }

    fn transactional_checkpoint_and_outbox(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        snapshot: &Value,
        outbox_messages: &[crate::types::OutboxMessage],
    ) -> Result<(), AceError> {
        let sb_tenant = tenant_to_sb(tenant_id);
        let cycle_id_str = cycle_id.as_u64().to_string();
        let now = current_millis();

        // 首先保存checkpoint
        let snapshot_doc = CycleSnapshotDoc {
            id: make_record_id(
                CycleSnapshotDoc::TABLE,
                &sb_tenant,
                &cycle_id_str,
            ),
            tenant: sb_tenant.clone(),
            cycle_id: cycle_id_str.clone(),
            snapshot_data: snapshot.clone(),
            created_at: now,
            updated_at: now,
        };

        // 构建outbox文档
        let outbox_docs: Result<Vec<OutboxMessageDoc>, AceError> = outbox_messages
            .iter()
            .map(|msg| {
                let event_id = msg.event_id.as_u64().to_string();
                let payload = serde_json::to_value(&msg.payload).map_err(|err| {
                    AceError::Persistence(format!("serialize outbox payload failed: {err}"))
                })?;

                Ok(OutboxMessageDoc {
                    id: make_record_id(
                        OutboxMessageDoc::TABLE,
                        &sb_tenant,
                        &event_id,
                    ),
                    tenant: sb_tenant.clone(),
                    event_id: event_id.clone(),
                    cycle_id: cycle_id_str.clone(),
                    payload,
                    status: "pending".to_string(),
                    created_at: now,
                    sent_at: None,
                })
            })
            .collect();

        let outbox_docs = outbox_docs?;

        let snapshot_repo = self.snapshot_repo.clone();
        let outbox_repo = self.outbox_repo.clone();

        // TODO: 这里应该使用真正的数据库事务
        // 目前的实现是先保存checkpoint，再保存outbox消息
        // 如果outbox保存失败，checkpoint已经保存，不会回滚
        // 在生产环境中应该使用SurrealDB的BEGIN TRANSACTION语法
        self.block_on(async move {
            // 保存checkpoint
            let mut snapshot_patch = serde_json::to_value(&snapshot_doc).map_err(|err| {
                AceError::Persistence(format!("serialize snapshot patch failed: {err}"))
            })?;
            if let serde_json::Value::Object(ref mut map) = snapshot_patch {
                map.remove("id");
                map.remove("tenant");
            }
            snapshot_repo
                .upsert_or_create(&sb_tenant, &cycle_id_str, snapshot_patch)
                .await
                .map_err(|err| {
                    AceError::Persistence(format!("persist checkpoint failed: {err}"))
                })?;

            // 保存outbox消息
            for doc in outbox_docs {
                let mut outbox_patch = serde_json::to_value(&doc).map_err(|err| {
                    AceError::Persistence(format!("serialize outbox patch failed: {err}"))
                })?;
                if let serde_json::Value::Object(ref mut map) = outbox_patch {
                    map.remove("id");
                    map.remove("tenant");
                }
                outbox_repo
                    .upsert_or_create(&sb_tenant, &doc.event_id, outbox_patch)
                    .await
                    .map_err(|err| {
                        AceError::Persistence(format!("persist outbox message failed: {err}"))
                    })?;
            }

            tracing::info!(
                "Transactional checkpoint and outbox saved: tenant={}, cycle={}, outbox_count={}",
                tenant_id.as_u64(),
                cycle_id.as_u64(),
                outbox_messages.len()
            );

            Ok(())
        })
    }

    fn list_pending_outbox(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        limit: usize,
    ) -> Result<Vec<crate::types::OutboxMessage>, AceError> {
        use crate::types::OutboxMessage;

        let limit = limit.clamp(1, 500);
        let sb_tenant = tenant_to_sb(tenant_id);
        let repo = self.outbox_repo.clone();

        self.block_on(async move {
            // 查询status="pending"的消息
            let filter = serde_json::json!({
                "status": "pending"
            });
            let params = QueryParams {
                filter,
                order_by: Some("created_at ASC".to_string()),
                limit: Some(limit as u32),
                cursor: None,
            };

            let page = repo
                .select(&sb_tenant, params)
                .await
                .map_err(|err| {
                    AceError::Persistence(format!("list pending outbox failed: {err}"))
                })?;

            let mut messages = Vec::with_capacity(page.items.len());
            for doc in page.items {
                match serde_json::from_value(doc.payload.clone()) {
                    Ok(payload) => {
                        let cycle_id_val = doc.cycle_id.parse::<u64>().unwrap_or(0);
                        let event_id_val = doc.event_id.parse::<u64>().unwrap_or(0);

                        messages.push(OutboxMessage {
                            cycle_id: soulseed_agi_core_models::AwarenessCycleId::from_raw_unchecked(cycle_id_val),
                            event_id: soulseed_agi_core_models::EventId::from_raw_unchecked(event_id_val),
                            payload,
                        });
                    }
                    Err(err) => {
                        tracing::warn!(
                            tenant = %doc.tenant.0,
                            event_id = %doc.event_id,
                            error = %err,
                            "skip outbox message due to deserialize failure"
                        );
                        continue;
                    }
                }
            }

            Ok(messages)
        })
    }

    fn mark_outbox_sent(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        event_ids: &[soulseed_agi_core_models::EventId],
    ) -> Result<(), AceError> {
        let sb_tenant = tenant_to_sb(tenant_id);
        let repo = self.outbox_repo.clone();
        let now = current_millis();
        let event_ids_str: Vec<String> = event_ids.iter().map(|id| id.as_u64().to_string()).collect();

        self.block_on(async move {
            for event_id_str in event_ids_str {
                let update_patch = serde_json::json!({
                    "status": "sent",
                    "sent_at": now
                });
                repo.upsert_or_create(&sb_tenant, &event_id_str, update_patch)
                    .await
                    .map_err(|err| {
                        AceError::Persistence(format!("mark outbox sent failed: {err}"))
                    })?;
            }

            tracing::info!(
                "Marked {} outbox messages as sent for tenant={}",
                event_ids.len(),
                tenant_id.as_u64()
            );

            Ok(())
        })
    }
}

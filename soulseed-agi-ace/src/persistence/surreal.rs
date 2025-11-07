#![cfg(feature = "persistence-surreal")]

use std::future::Future;
use std::sync::Arc;

use sb_storage::model::{Entity, make_record_id};
use sb_storage::prelude::Repository;
use sb_storage::surreal::config::SurrealConfig;
use sb_storage::surreal::datastore::SurrealDatastore;
use sb_types::prelude::TenantId as SbTenantId;
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
    SbTenantId::from(tenant.as_u64().to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DialogueEventDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    pub tenant: String,
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

impl Entity for DialogueEventDoc {
    const TABLE: &'static str = "ace_dialogue_event";
    type Key = String;

    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AwarenessEventDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    pub tenant: String,
    pub event_id: String,
    pub cycle_id: String,
    pub event_type: String,
    pub occurred_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_cycle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collab_scope_id: Option<String>,
    pub payload: Value,
    pub created_at: i64,
}

impl Entity for AwarenessEventDoc {
    const TABLE: &'static str = "ace_awareness_event";
    type Key = String;

    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CycleSnapshotDoc {
    #[serde(skip_deserializing)]
    pub id: String,
    pub tenant: String,
    pub cycle_id: String,
    pub snapshot: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Entity for CycleSnapshotDoc {
    const TABLE: &'static str = "ace_cycle_snapshot";
    type Key = String;

    fn id(&self) -> &str {
        &self.id
    }
}

pub struct SurrealPersistence {
    handle: tokio::runtime::Handle,
    owned_runtime: Option<tokio::runtime::Runtime>,
    dialogue_repo: Arc<sb_storage::surreal::repo::SurrealRepository<DialogueEventDoc>>,
    awareness_repo: Arc<sb_storage::surreal::repo::SurrealRepository<AwarenessEventDoc>>,
    snapshot_repo: Arc<sb_storage::surreal::repo::SurrealRepository<CycleSnapshotDoc>>,
    datastore: SurrealDatastore,
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
            Arc<sb_storage::surreal::repo::SurrealRepository<DialogueEventDoc>>,
            Arc<sb_storage::surreal::repo::SurrealRepository<AwarenessEventDoc>>,
            Arc<sb_storage::surreal::repo::SurrealRepository<CycleSnapshotDoc>>,
        ),
        AceError,
    > {
        let datastore = SurrealDatastore::connect(datastore_cfg.clone())
            .await
            .map_err(|err| AceError::Persistence(format!("surreal connect failed: {err}")))?;
        let dialogue_repo = Arc::new(sb_storage::surreal::repo::SurrealRepository::<
            DialogueEventDoc,
        >::new(datastore.clone()));
        let awareness_repo = Arc::new(sb_storage::surreal::repo::SurrealRepository::<
            AwarenessEventDoc,
        >::new(datastore.clone()));
        let snapshot_repo = Arc::new(sb_storage::surreal::repo::SurrealRepository::<
            CycleSnapshotDoc,
        >::new(datastore.clone()));
        Ok((datastore, dialogue_repo, awareness_repo, snapshot_repo))
    }

    pub fn new_with_handle(
        handle: tokio::runtime::Handle,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo) =
            handle.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            owned_runtime: None,
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
            datastore,
        })
    }

    /// 异步初始化（用于已在 tokio runtime 中的场景）
    pub async fn new_async(config: SurrealPersistenceConfig) -> Result<Self, AceError> {
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo) =
            Self::init_components(config.datastore).await?;

        let handle = tokio::runtime::Handle::current();

        Ok(Self {
            handle,
            owned_runtime: None,
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
            datastore,
        })
    }

    pub fn new_with_owned_runtime(
        runtime: tokio::runtime::Runtime,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let handle = runtime.handle().clone();
        let (datastore, dialogue_repo, awareness_repo, snapshot_repo) =
            runtime.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            owned_runtime: Some(runtime),
            dialogue_repo,
            awareness_repo,
            snapshot_repo,
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
        let tenant = emission.anchor.tenant_id.as_u64().to_string();
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
                &SbTenantId::from(tenant.clone()),
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
        let tenant = emission.anchor.tenant_id.as_u64().to_string();
        let cycle_id = emission.cycle_id.as_u64().to_string();
        let event_id = event.event_id.as_u64().to_string();
        let event_type = serde_json::to_value(event.event_type)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or_else(|| format!("{:?}", event.event_type));
        let payload = serde_json::to_value(event).map_err(|err| {
            AceError::Persistence(format!("serialize awareness event failed: {err}"))
        })?;
        Ok(AwarenessEventDoc {
            id: make_record_id(
                AwarenessEventDoc::TABLE,
                &SbTenantId::from(tenant.clone()),
                &event_id,
            ),
            tenant,
            event_id,
            cycle_id,
            event_type,
            occurred_at: event.occurred_at_ms,
            parent_cycle_id: event.parent_cycle_id.map(|id| id.as_u64().to_string()),
            collab_scope_id: event.collab_scope_id.clone(),
            payload,
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
            let _patch_json = serde_json::to_string(&patch)
                .map_err(|err| AceError::Persistence(format!("dialogue patch stringify failed: {err}")))?;
            if let serde_json::Value::Object(ref mut map) = patch {
                map.remove("id");
            }
            let patch_for_log = patch.clone();
            dialogue_repo
                .upsert_returning_none(&sb_tenant, dialogue.event_id.as_str(), patch)
                .await
                .map_err(|err| {
                    let err_obj = err.into_inner();
                    let public = err_obj.to_public();
                    let audit = err_obj.to_audit();
                    let detail = audit
                        .message_dev
                        .unwrap_or(&public.message)
                        .to_string();
                    tracing::error!(
                        code = %public.code,
                        message = %public.message,
                        message_dev = audit.message_dev.unwrap_or(""),
                        correlation_id = audit.correlation_id.unwrap_or(""),
                        meta = ?audit.meta,
                        tenant = %dialogue.tenant,
                        event_id = %dialogue.event_id,
                        patch = ?patch_for_log,
                        "persist dialogue event failed"
                    );
                    AceError::Persistence(format!("persist dialogue event failed: {}", detail))
                })?;

            for doc in awareness {
                let mut patch = serde_json::to_value(&doc).map_err(|err| {
                    AceError::Persistence(format!("serialize awareness patch failed: {err}"))
                })?;
                let _patch_json = serde_json::to_string(&patch)
                    .map_err(|err| AceError::Persistence(format!("awareness patch stringify failed: {err}")))?;
                if let serde_json::Value::Object(ref mut map) = patch {
                    map.remove("id");
                }
                let patch_for_log = patch.clone();
                awareness_repo
                    .upsert_returning_none(&sb_tenant, doc.event_id.as_str(), patch)
                    .await
                    .map_err(|err| {
                        let err_obj = err.into_inner();
                        let public = err_obj.to_public();
                        let audit = err_obj.to_audit();
                        let detail = audit
                            .message_dev
                            .unwrap_or(&public.message)
                            .to_string();
                        tracing::error!(
                            code = %public.code,
                            message = %public.message,
                            message_dev = audit.message_dev.unwrap_or(""),
                            correlation_id = audit.correlation_id.unwrap_or(""),
                            meta = ?audit.meta,
                            tenant = %doc.tenant,
                            event_id = %doc.event_id,
                            patch = ?patch_for_log,
                            "persist awareness event failed"
                        );
                        AceError::Persistence(format!("persist awareness event failed: {}", detail))
                    })?;
            }

            Ok::<_, AceError>(())
        })
    }

    fn persist_cycle_snapshot(
        &self,
        tenant_id: soulseed_agi_core_models::TenantId,
        cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        snapshot: &Value,
    ) -> Result<(), AceError> {
        // Schema 已通过 migrations/surreal/001_soulseed_init.sql 初始化

        let tenant = tenant_id.as_u64().to_string();
        let cycle_id_str = cycle_id.as_u64().to_string();
        let now = current_millis();

        let doc = CycleSnapshotDoc {
            id: make_record_id(
                CycleSnapshotDoc::TABLE,
                &SbTenantId::from(tenant.clone()),
                &cycle_id_str,
            ),
            tenant: tenant.clone(),
            cycle_id: cycle_id_str.clone(),
            snapshot: snapshot.clone(),
            created_at: now,
            updated_at: now,
        };

        let sb_tenant = tenant_to_sb(tenant_id);
        let snapshot_repo = self.snapshot_repo.clone();

        self.block_on(async move {
            let mut patch = serde_json::to_value(&doc).map_err(|err| {
                AceError::Persistence(format!("serialize snapshot patch failed: {err}"))
            })?;
            let _patch_json = serde_json::to_string(&patch)
                .map_err(|err| AceError::Persistence(format!("snapshot patch stringify failed: {err}")))?;
            if let serde_json::Value::Object(ref mut map) = patch {
                map.remove("id");
            }
            let patch_for_log = patch.clone();
            snapshot_repo
                .upsert_returning_none(&sb_tenant, &cycle_id_str, patch)
                .await
                .map_err(|err| {
                    let err_obj = err.into_inner();
                    let public = err_obj.to_public();
                    let audit = err_obj.to_audit();
                    let detail = audit
                        .message_dev
                        .unwrap_or(&public.message)
                        .to_string();
                    tracing::error!(
                        code = %public.code,
                        message = %public.message,
                        message_dev = audit.message_dev.unwrap_or(""),
                        correlation_id = audit.correlation_id.unwrap_or(""),
                        meta = ?audit.meta,
                        tenant = %tenant,
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
        let sb_tenant = tenant_to_sb(tenant_id);
        let snapshot_repo = self.snapshot_repo.clone();

        self.block_on(async move {
            let result = snapshot_repo
                .get(&sb_tenant, &cycle_id_str)
                .await
                .map_err(|err| {
                    AceError::Persistence(format!("load cycle snapshot failed: {err}"))
                })?;

            Ok(result.map(|doc| doc.snapshot))
        })
    }
}

#![cfg(feature = "persistence-surreal")]

use std::future::Future;
use std::sync::{Arc, Mutex};

use sb_storage::model::{Entity, make_record_id};
use sb_storage::prelude::{NamedArgs, Repository};
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

const SCHEMA: [&str; 2] = [
    r#"
    DEFINE TABLE ace_dialogue_event SCHEMAFULL;
    DEFINE FIELD tenant ON ace_dialogue_event TYPE string;
    DEFINE FIELD event_id ON ace_dialogue_event TYPE string;
    DEFINE FIELD cycle_id ON ace_dialogue_event TYPE string;
    DEFINE FIELD occurred_at ON ace_dialogue_event TYPE number;
    DEFINE FIELD lane ON ace_dialogue_event TYPE string;
    DEFINE FIELD payload ON ace_dialogue_event TYPE object;
    DEFINE FIELD manifest_digest ON ace_dialogue_event TYPE option<string>;
    DEFINE FIELD explain_fingerprint ON ace_dialogue_event TYPE option<string>;
    DEFINE FIELD created_at ON ace_dialogue_event TYPE number;
    DEFINE INDEX ace_dialogue_event_lookup ON TABLE ace_dialogue_event FIELDS tenant, event_id UNIQUE;
    DEFINE INDEX ace_dialogue_event_cycle ON TABLE ace_dialogue_event FIELDS tenant, cycle_id;
    "#,
    r#"
    DEFINE TABLE ace_awareness_event SCHEMAFULL;
    DEFINE FIELD tenant ON ace_awareness_event TYPE string;
    DEFINE FIELD event_id ON ace_awareness_event TYPE string;
    DEFINE FIELD cycle_id ON ace_awareness_event TYPE string;
    DEFINE FIELD event_type ON ace_awareness_event TYPE string;
    DEFINE FIELD occurred_at ON ace_awareness_event TYPE number;
    DEFINE FIELD payload ON ace_awareness_event TYPE object;
    DEFINE FIELD parent_cycle_id ON ace_awareness_event TYPE option<string>;
    DEFINE FIELD collab_scope_id ON ace_awareness_event TYPE option<string>;
    DEFINE FIELD created_at ON ace_awareness_event TYPE number;
    DEFINE INDEX ace_awareness_event_lookup ON TABLE ace_awareness_event FIELDS tenant, event_id UNIQUE;
    DEFINE INDEX ace_awareness_event_cycle ON TABLE ace_awareness_event FIELDS tenant, cycle_id;
    DEFINE INDEX ace_awareness_event_type ON TABLE ace_awareness_event FIELDS tenant, event_type;
    "#,
];

fn tenant_to_sb(tenant: soulseed_agi_core_models::TenantId) -> SbTenantId {
    SbTenantId::from(tenant.as_u64().to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DialogueEventDoc {
    pub id: String,
    pub tenant: String,
    pub event_id: String,
    pub cycle_id: String,
    pub occurred_at: i64,
    pub lane: String,
    pub payload: Value,
    pub manifest_digest: Option<String>,
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
    pub id: String,
    pub tenant: String,
    pub event_id: String,
    pub cycle_id: String,
    pub event_type: String,
    pub occurred_at: i64,
    pub parent_cycle_id: Option<String>,
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

pub struct SurrealPersistence {
    handle: tokio::runtime::Handle,
    owned_runtime: Option<tokio::runtime::Runtime>,
    dialogue_repo: Arc<sb_storage::surreal::repo::SurrealRepository<DialogueEventDoc>>,
    awareness_repo: Arc<sb_storage::surreal::repo::SurrealRepository<AwarenessEventDoc>>,
    datastore: SurrealDatastore,
    migrations_ran: Mutex<bool>,
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
        Ok((datastore, dialogue_repo, awareness_repo))
    }

    pub(crate) fn new_with_handle(
        handle: tokio::runtime::Handle,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let (datastore, dialogue_repo, awareness_repo) =
            handle.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            owned_runtime: None,
            dialogue_repo,
            awareness_repo,
            datastore,
            migrations_ran: Mutex::new(false),
        })
    }

    pub(crate) fn new_with_owned_runtime(
        runtime: tokio::runtime::Runtime,
        config: SurrealPersistenceConfig,
    ) -> Result<Self, AceError> {
        let handle = runtime.handle().clone();
        let (datastore, dialogue_repo, awareness_repo) =
            runtime.block_on(Self::init_components(config.datastore))?;

        Ok(Self {
            handle,
            owned_runtime: Some(runtime),
            dialogue_repo,
            awareness_repo,
            datastore,
            migrations_ran: Mutex::new(false),
        })
    }

    fn ensure_schema(&self) -> Result<(), AceError> {
        let mut guard = self.migrations_ran.lock().unwrap();
        if *guard {
            return Ok(());
        }
        let datastore = self.datastore.clone();
        self.block_on(async move {
            let pool = datastore.pool();
            for stmt in SCHEMA.iter() {
                pool.run_raw(stmt, &NamedArgs::default())
                    .await
                    .map_err(|err| AceError::Persistence(format!("apply schema failed: {err}")))?;
            }
            Ok::<_, AceError>(())
        })?;
        *guard = true;
        Ok(())
    }

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
        self.ensure_schema()?;

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
            let patch = serde_json::to_value(&dialogue).map_err(|err| {
                AceError::Persistence(format!("serialize dialogue patch failed: {err}"))
            })?;
            dialogue_repo
                .upsert(&sb_tenant, dialogue.event_id.as_str(), patch, None)
                .await
                .map_err(|err| {
                    AceError::Persistence(format!("persist dialogue event failed: {err}"))
                })?;

            for doc in awareness {
                let patch = serde_json::to_value(&doc).map_err(|err| {
                    AceError::Persistence(format!("serialize awareness patch failed: {err}"))
                })?;
                awareness_repo
                    .upsert(&sb_tenant, doc.event_id.as_str(), patch, None)
                    .await
                    .map_err(|err| {
                        AceError::Persistence(format!("persist awareness event failed: {err}"))
                    })?;
            }

            Ok::<_, AceError>(())
        })
    }
}

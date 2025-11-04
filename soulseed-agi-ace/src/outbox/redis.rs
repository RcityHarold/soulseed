#![cfg(feature = "outbox-redis")]

use std::sync::{Arc, Mutex};

use sb_storage::surreal::config::SurrealConfig;
use sb_storage::surreal::datastore::SurrealDatastore;
use sb_tx::a2a::NoopA2AHooks;
use sb_tx::config::TxConfig;
use sb_tx::observe::{NoopTxMetrics, TxMetrics};
use sb_tx::outbox::{Dispatcher, OutboxStore};
use sb_tx::surreal::{SurrealTxStore, apply_migrations};
use sb_tx::transport::redis::{RedisTransport, RedisTransportConfig};
use sb_tx::util::now_ms;
use sb_tx::worker::{TxRuntimeHandles, spawn_runtime};
use sb_types::prelude::{Id as SbId, TenantId as SbTenantId};
use soulseed_agi_core_models::TenantId;
use soulseed_agi_core_models::awareness::AwarenessEvent;
use tokio::runtime::{Builder, Runtime};

use crate::errors::AceError;
use crate::outbox::{OutboxForwarder, OutboxMessage};

#[derive(Clone, Debug)]
pub struct RedisForwarderConfig {
    pub surreal: SurrealConfig,
    pub tx: TxConfig,
    pub redis: RedisTransportConfig,
    pub tenant: TenantId,
}

impl Default for RedisForwarderConfig {
    fn default() -> Self {
        Self {
            surreal: SurrealConfig::default(),
            tx: TxConfig::default(),
            redis: RedisTransportConfig::default(),
            tenant: TenantId::from_raw_unchecked(1),
        }
    }
}

pub struct RedisOutboxForwarder {
    runtime: Runtime,
    store: SurrealTxStore,
    tenant: u64,
    sb_tenant: SbTenantId,
    handles: Mutex<Option<TxRuntimeHandles>>,
}

impl RedisOutboxForwarder {
    pub fn new(config: RedisForwarderConfig) -> Result<Self, AceError> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| AceError::Outbox(format!("tokio runtime init failed: {err}")))?;

        let tenant_u64 = config.tenant.as_u64();
        let sb_tenant = SbTenantId::from(tenant_u64.to_string());
        let tenant_for_runtime = sb_tenant.clone();

        let (store, handles) = runtime.block_on(async move {
            let datastore = SurrealDatastore::connect(config.surreal.clone())
                .await
                .map_err(|err| AceError::Outbox(format!("surreal connect failed: {err}")))?;

            apply_migrations(&datastore)
                .await
                .map_err(|err| AceError::Outbox(format!("apply migrations failed: {err}")))?;

            let metrics: Arc<dyn TxMetrics> = Arc::new(NoopTxMetrics);
            let qos = config.tx.build_budget_guard();
            let store = SurrealTxStore::with_config(
                datastore,
                config.tx.clone(),
                metrics.clone(),
                qos.clone(),
                Arc::new(NoopA2AHooks),
            );

            let transport = RedisTransport::new(config.redis.clone())
                .map_err(|err| AceError::Outbox(format!("redis transport init failed: {err}")))?;

            let backoff = Arc::new(
                config
                    .tx
                    .outbox
                    .backoff
                    .clone()
                    .into_retry_policy_with_attempts(config.tx.outbox.max_attempts),
            );

            let dispatcher = Dispatcher::new(
                transport,
                store.clone(),
                "redis-forwarder",
                config.tx.outbox.max_attempts,
                config.tx.outbox.lease_ms,
                config.tx.outbox.batch,
                backoff,
                config.tx.outbox.group_by_dispatch_key,
                Some(store.dead_store()),
                metrics,
                qos,
            );

            let handles = spawn_runtime(
                Some(dispatcher),
                Some(store.dead_store()),
                tenant_for_runtime,
                &config.tx.worker,
                config.tx.dead_letter.retention_ms,
            );

            Ok::<_, AceError>((store, handles))
        })?;

        Ok(Self {
            runtime,
            store,
            tenant: tenant_u64,
            sb_tenant,
            handles: Mutex::new(Some(handles)),
        })
    }

    fn event_topic(event: &AwarenessEvent) -> String {
        serde_json::to_value(event.event_type)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_owned()))
            .unwrap_or_else(|| format!("{:?}", event.event_type))
    }
}

impl Drop for RedisOutboxForwarder {
    fn drop(&mut self) {
        if let Some(handles) = self.handles.lock().unwrap().take() {
            let _ = self.runtime.block_on(async { handles.shutdown().await });
        }
    }
}

impl OutboxForwarder for RedisOutboxForwarder {
    fn forward(&self, tenant: TenantId, messages: &[OutboxMessage]) -> Result<(), AceError> {
        if messages.is_empty() {
            return Ok(());
        }
        if tenant.as_u64() != self.tenant {
            return Err(AceError::Outbox(format!(
                "forwarder bound to tenant {}, received {}",
                self.tenant,
                tenant.as_u64()
            )));
        }

        let sb_tenant = self.sb_tenant.clone();
        let mut new_messages = Vec::with_capacity(messages.len());
        for message in messages {
            let payload = serde_json::to_value(&message.payload).map_err(|err| {
                AceError::Outbox(format!("serialize awareness payload failed: {err}"))
            })?;

            let event_key = Self::event_topic(&message.payload);
            let topic = format!("redis://awareness/{event_key}");
            let new_msg = sb_tx::model::NewOutboxMessage {
                id: SbId::from(format!(
                    "awareness:{}:{}",
                    message.cycle_id.as_u64(),
                    message.event_id.as_u64()
                )),
                tenant: sb_tenant.clone(),
                envelope_id: SbId::from(format!("cycle:{}", message.cycle_id.as_u64())),
                topic,
                payload,
                not_before: Some(now_ms()),
                dispatch_key: Some(event_key),
            };
            new_messages.push(new_msg);
        }

        let store = self.store.clone();
        self.runtime.block_on(async move {
            for msg in new_messages {
                store
                    .enqueue(msg)
                    .await
                    .map_err(|err| AceError::Outbox(format!("enqueue failed: {err}")))?;
            }
            Ok::<_, AceError>(())
        })
    }
}

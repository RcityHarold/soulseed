use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{OutboxEnvelope, OutboxMessage};
use soulseed_agi_core_models::TenantId;

pub trait OutboxForwarder: Send + Sync {
    fn forward(&self, tenant: TenantId, messages: &[OutboxMessage]) -> Result<(), AceError>;
}

#[derive(Default)]
struct OutboxState {
    queues: HashMap<u64, VecDeque<OutboxMessage>>,
    fail_next_commit: bool,
}

struct OutboxServiceInner {
    state: Mutex<OutboxState>,
    forwarder: Option<Arc<dyn OutboxForwarder>>,
}

impl Default for OutboxServiceInner {
    fn default() -> Self {
        Self {
            state: Mutex::new(OutboxState::default()),
            forwarder: None,
        }
    }
}

#[derive(Clone)]
pub struct OutboxService {
    inner: Arc<OutboxServiceInner>,
}

impl Default for OutboxService {
    fn default() -> Self {
        Self {
            inner: Arc::new(OutboxServiceInner::default()),
        }
    }
}

impl OutboxService {
    pub fn with_forwarder(forwarder: Arc<dyn OutboxForwarder>) -> Self {
        Self {
            inner: Arc::new(OutboxServiceInner {
                state: Mutex::new(OutboxState::default()),
                forwarder: Some(forwarder),
            }),
        }
    }

    pub fn enqueue(&self, envelope: OutboxEnvelope) -> Result<(), AceError> {
        let mut reservation = self.reserve(envelope);
        reservation.commit()
    }

    pub fn reserve(&self, envelope: OutboxEnvelope) -> OutboxReservation {
        OutboxReservation {
            inner: Arc::clone(&self.inner),
            tenant: envelope.tenant_id.into_inner(),
            messages: Some(envelope.messages),
            committed: false,
        }
    }

    pub fn dequeue(&self, tenant_id: TenantId, max: usize) -> Vec<OutboxMessage> {
        let mut guard = self.inner.state.lock().unwrap();
        let queue = guard.queues.entry(tenant_id.into_inner()).or_default();
        let mut drained = Vec::new();
        for _ in 0..max {
            if let Some(msg) = queue.pop_front() {
                drained.push(msg);
            } else {
                break;
            }
        }
        drained
    }

    pub fn peek(&self, tenant_id: TenantId) -> Vec<OutboxMessage> {
        let guard = self.inner.state.lock().unwrap();
        guard
            .queues
            .get(&tenant_id.into_inner())
            .map(|queue| queue.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn fail_next_commit(&self) {
        let mut guard = self.inner.state.lock().unwrap();
        guard.fail_next_commit = true;
    }
}

pub struct OutboxReservation {
    inner: Arc<OutboxServiceInner>,
    tenant: u64,
    messages: Option<Vec<OutboxMessage>>,
    committed: bool,
}

impl OutboxReservation {
    pub fn commit(&mut self) -> Result<(), AceError> {
        if self.committed {
            return Ok(());
        }
        let messages = self.messages.take().unwrap_or_default();
        let to_forward = if self.inner.forwarder.is_some() {
            Some(messages.clone())
        } else {
            None
        };

        {
            let mut guard = self.inner.state.lock().unwrap();
            if guard.fail_next_commit {
                guard.fail_next_commit = false;
                self.messages = Some(messages);
                return Err(AceError::Outbox("commit_failed".into()));
            }
            let queue = guard.queues.entry(self.tenant).or_default();
            queue.extend(messages.into_iter());
        }

        if let (Some(forwarder), Some(batch)) = (self.inner.forwarder.as_ref(), to_forward.as_ref())
        {
            let tenant = TenantId::from_raw_unchecked(self.tenant);
            forwarder.forward(tenant, batch)?;
        }

        self.committed = true;
        Ok(())
    }
}

#[cfg(feature = "outbox-redis")]
pub mod redis;

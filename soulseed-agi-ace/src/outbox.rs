use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{OutboxEnvelope, OutboxMessage};
use soulseed_agi_core_models::TenantId;

#[derive(Default)]
struct OutboxState {
    queues: HashMap<u64, VecDeque<OutboxMessage>>,
    fail_next_commit: bool,
}

#[derive(Clone, Default)]
pub struct OutboxService {
    inner: Arc<Mutex<OutboxState>>,
}

impl OutboxService {
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
        let mut guard = self.inner.lock().unwrap();
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

    pub fn fail_next_commit(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.fail_next_commit = true;
    }
}

pub struct OutboxReservation {
    inner: Arc<Mutex<OutboxState>>,
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
        let mut guard = self.inner.lock().unwrap();
        if guard.fail_next_commit {
            guard.fail_next_commit = false;
            self.messages = Some(messages);
            return Err(AceError::Outbox("commit_failed".into()));
        }
        let queue = guard.queues.entry(self.tenant).or_default();
        queue.extend(messages.into_iter());
        self.committed = true;
        Ok(())
    }
}

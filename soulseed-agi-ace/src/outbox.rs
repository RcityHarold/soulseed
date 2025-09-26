use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{OutboxEnvelope, OutboxMessage};
use soulseed_agi_core_models::TenantId;

#[derive(Default)]
struct OutboxState {
    queues: HashMap<u64, VecDeque<OutboxMessage>>,
}

#[derive(Clone, Default)]
pub struct OutboxService {
    inner: Arc<Mutex<OutboxState>>,
}

impl OutboxService {
    pub fn enqueue(&self, envelope: OutboxEnvelope) -> Result<(), AceError> {
        let mut guard = self.inner.lock().unwrap();
        let queue = guard
            .queues
            .entry(envelope.tenant_id.into_inner())
            .or_default();
        queue.extend(envelope.messages.into_iter());
        Ok(())
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
}

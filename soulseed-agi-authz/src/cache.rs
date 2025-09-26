use std::collections::HashMap;
use std::sync::Mutex;

use crate::{policy::Policy, AccessTicket, ResourceUrn, TenantId};

pub trait PolicyStore: Send + Sync {
    fn policies_for_tenant(&self, tenant: TenantId) -> Vec<Policy>;
}

pub trait TicketStore: Send + Sync {
    fn ticket_for(&self, tenant: TenantId, ticket_id: &str) -> Option<AccessTicket>;
}

#[derive(Default)]
pub struct InMemoryPolicyStore {
    inner: Mutex<HashMap<TenantId, Vec<Policy>>>,
}

impl InMemoryPolicyStore {
    pub fn push(&self, tenant: TenantId, policy: Policy) {
        let mut guard = self.inner.lock().unwrap();
        guard.entry(tenant).or_default().push(policy);
    }
}

impl PolicyStore for InMemoryPolicyStore {
    fn policies_for_tenant(&self, tenant: TenantId) -> Vec<Policy> {
        self.inner
            .lock()
            .unwrap()
            .get(&tenant)
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Default)]
pub struct InMemoryTicketStore {
    inner: Mutex<HashMap<(TenantId, String), AccessTicket>>,
}

impl InMemoryTicketStore {
    pub fn insert(&self, ticket: AccessTicket) {
        let key = (ticket.tenant_id, ticket.ticket_id.clone());
        self.inner.lock().unwrap().insert(key, ticket);
    }
}

impl TicketStore for InMemoryTicketStore {
    fn ticket_for(&self, tenant: TenantId, ticket_id: &str) -> Option<AccessTicket> {
        self.inner
            .lock()
            .unwrap()
            .get(&(tenant, ticket_id.to_string()))
            .cloned()
    }
}

pub fn resource_scope_specificity(urn: &ResourceUrn) -> usize {
    urn.0.split(':').count()
}

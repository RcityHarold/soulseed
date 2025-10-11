use std::cmp::Ordering;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceSpecificity {
    depth: usize,
    literal_len: usize,
    wildcard_count: usize,
    trailing_wildcard: bool,
}

impl ResourceSpecificity {
    fn from_urn(urn: &ResourceUrn) -> Self {
        let raw = urn.0.as_str();
        let segments = raw
            .split(|c| c == ':' || c == '/')
            .filter(|segment| !segment.is_empty())
            .count();
        let wildcard_count = raw.chars().filter(|c| *c == '*').count();
        let literal_len = raw.len().saturating_sub(wildcard_count);
        let trailing_wildcard = raw.ends_with('*');
        Self {
            depth: segments,
            literal_len,
            wildcard_count,
            trailing_wildcard,
        }
    }
}

impl Ord for ResourceSpecificity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.depth
            .cmp(&other.depth)
            .then_with(|| self.literal_len.cmp(&other.literal_len))
            .then_with(|| other.wildcard_count.cmp(&self.wildcard_count))
            .then_with(|| (!self.trailing_wildcard).cmp(&(!other.trailing_wildcard)))
    }
}

impl PartialOrd for ResourceSpecificity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn resource_scope_specificity(urn: &ResourceUrn) -> ResourceSpecificity {
    ResourceSpecificity::from_urn(urn)
}

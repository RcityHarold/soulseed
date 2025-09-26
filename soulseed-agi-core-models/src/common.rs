use crate::ids::{AIId, GroupId, HumanId};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use time::OffsetDateTime;

#[cfg(all(feature = "uuidv7", feature = "id-u64"))]
compile_error!("features 'uuidv7' and 'id-u64' cannot be enabled together");

#[cfg(feature = "uuidv7")]
pub type EnvelopeId = uuid::Uuid;

#[cfg(all(not(feature = "uuidv7"), feature = "id-u64"))]
pub type EnvelopeId = u128;

#[cfg(all(not(feature = "uuidv7"), not(feature = "id-u64")))]
pub type EnvelopeId = uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

impl Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub String);

impl Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvelopeHead {
    pub envelope_id: EnvelopeId,
    pub trace_id: TraceId,
    pub correlation_id: CorrelationId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Subject {
    Human(HumanId),
    AI(AIId),
    System,
    Group(GroupId),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubjectRef {
    pub kind: Subject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessClass {
    Public,
    Internal,
    Restricted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_digest_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingMeta {
    pub model: String,
    pub dim: u16,
    pub ts: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub schema_v: u16,
    pub created_at: OffsetDateTime,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ModelError {
    #[error("missing required field: {0}")]
    Missing(&'static str),
    #[error("invariant violated: {0}")]
    Invariant(&'static str),
}

#[cfg(feature = "uuidv7")]
pub fn new_envelope_id() -> EnvelopeId {
    uuid::Uuid::now_v7()
}

#[cfg(all(not(feature = "uuidv7"), feature = "id-u64"))]
pub fn new_envelope_id() -> EnvelopeId {
    use std::sync::atomic::{AtomicU128, Ordering};
    static NEXT: AtomicU128 = AtomicU128::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(all(not(feature = "uuidv7"), not(feature = "id-u64")))]
pub fn new_envelope_id() -> EnvelopeId {
    uuid::Uuid::now_v7()
}

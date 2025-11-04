use crate::errors::AceError;
use crate::types::CycleEmission;

pub trait AcePersistence: Send + Sync {
    fn persist_cycle(&self, emission: &CycleEmission) -> Result<(), AceError>;
}

#[cfg(feature = "persistence-surreal")]
pub mod surreal;

#[cfg(feature = "persistence-surreal")]
pub use surreal::{SurrealPersistence, SurrealPersistenceConfig};

use crate::{errors::ContextError, traits::PointerValidator, types::EvidencePointer};

pub struct PointerValidatorMock;

impl PointerValidator for PointerValidatorMock {
    fn validate(&self, pointer: &EvidencePointer) -> Result<(), ContextError> {
        if !pointer.checksum.starts_with("sha256:") {
            return Err(ContextError::PointerInvalid("checksum".into()));
        }
        if pointer.access_policy.is_empty() {
            return Err(ContextError::PointerInvalid("access_policy".into()));
        }
        Ok(())
    }
}

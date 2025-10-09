use crate::{errors::ContextError, traits::PointerValidator, types::EvidencePointer};

pub struct PointerValidatorMock;

impl PointerValidator for PointerValidatorMock {
    fn validate(&self, pointer: &EvidencePointer) -> Result<(), ContextError> {
        let digest = pointer
            .digest_sha256
            .as_deref()
            .ok_or_else(|| ContextError::PointerInvalid("digest_missing".into()))?;
        if !digest.starts_with("sha256:") {
            return Err(ContextError::PointerInvalid("digest_sha256".into()));
        }
        if pointer
            .access_policy
            .as_deref()
            .map(|policy| policy.is_empty())
            .unwrap_or(true)
        {
            return Err(ContextError::PointerInvalid("access_policy".into()));
        }
        Ok(())
    }
}

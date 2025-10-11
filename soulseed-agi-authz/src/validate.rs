use crate::errors::AuthzError;
use crate::{AccessClass, Anchor};

pub fn validate_anchor(anchor: &Anchor) -> Result<(), AuthzError> {
    if anchor.tenant_id.as_u64() == 0 {
        return Err(AuthzError::TenantForbidden);
    }
    if anchor.access_class == AccessClass::Restricted && anchor.provenance.is_none() {
        return Err(AuthzError::PrivacyRestricted);
    }
    if let Some(supersedes) = anchor.supersedes.as_ref() {
        if supersedes == &anchor.envelope_id {
            return Err(AuthzError::InvalidRequest("anchor_supersedes_self".into()));
        }
    }
    Ok(())
}

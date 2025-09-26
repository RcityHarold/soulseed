use crate::errors::AuthzError;
use crate::{AccessClass, Anchor};

pub fn validate_anchor(anchor: &Anchor) -> Result<(), AuthzError> {
    if anchor.tenant_id.0 == 0 {
        return Err(AuthzError::TenantForbidden);
    }
    if anchor.access_class == AccessClass::Restricted && anchor.provenance.is_none() {
        return Err(AuthzError::PrivacyRestricted);
    }
    Ok(())
}

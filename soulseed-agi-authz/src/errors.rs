use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthzError {
    #[error("tenant context missing or invalid")]
    TenantForbidden,
    #[error("privacy restricted: provenance required for restricted payload")]
    PrivacyRestricted,
    #[error("resource urn invalid")]
    InvalidResource,
    #[error("quota failure: {0}")]
    QuotaFailure(String),
}

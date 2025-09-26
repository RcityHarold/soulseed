use crate::{Subject, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub name: String,
    pub limit: i64,
    pub window: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Cost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_cost_usd: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointBalance {
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub hp: i64,
    pub ap: i64,
}

impl PointBalance {
    pub fn new(tenant_id: TenantId, subject: Subject, hp: i64, ap: i64) -> Self {
        Self {
            tenant_id,
            subject,
            hp,
            ap,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voucher {
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub voucher_id: String,
    pub description: String,
    pub valid_until_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gift {
    pub tenant_id: TenantId,
    pub subject: Subject,
    pub gift_id: String,
    pub name: String,
    pub value: f32,
    pub issued_at_ms: i64,
}

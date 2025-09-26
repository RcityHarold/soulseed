use crate::{AIId, AccessClass, HumanId, ModelError, Provenance, SoulState, TenantId};
use serde::{Deserialize, Serialize};

#[cfg(feature = "strict-privacy")]
fn default_restricted() -> AccessClass {
    AccessClass::Restricted
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HumanProfile {
    pub tenant_id: TenantId,
    pub user_id: HumanId,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,

    pub username: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interests: Vec<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AIProfile {
    pub tenant_id: TenantId,
    pub ai_id: AIId,
    #[cfg_attr(feature = "strict-privacy", serde(default = "default_restricted"))]
    pub access_class: AccessClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub core_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrative: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning_narrative: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationships_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soul_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_birthday_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    pub soul_state: SoulState,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
}

impl HumanProfile {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.username.trim().is_empty() {
            return Err(ModelError::Missing("username"));
        }
        if self.nickname.trim().is_empty() {
            return Err(ModelError::Missing("nickname"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }
}

impl AIProfile {
    pub fn validate(&self) -> Result<(), ModelError> {
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }
}

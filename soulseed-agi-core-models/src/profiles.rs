#[cfg(feature = "vectors-extra")]
use crate::vectors_ext::ExtraVectors;
use crate::{
    enums::{CorePersonalityKind, MembershipLevel, RelationshipStatus, SubscriptionStatus},
    resources::PointBalance,
    AIId, AccessClass, HumanId, ModelError, Provenance, SoulState, TenantId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HumanQuota {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yearly_group_creation: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_companion_slots: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_wakeup_slots: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightCoinBalance {
    pub current: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_earned: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_spent: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoucherSlot {
    pub voucher_id: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoucherInventory {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vouchers: Vec<VoucherSlot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointBalanceSnapshot {
    pub hp: i64,
    pub ap: i64,
}

impl From<PointBalance> for PointBalanceSnapshot {
    fn from(balance: PointBalance) -> Self {
        Self {
            hp: balance.hp,
            ap: balance.ap,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrequencyComponent {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValueFrequencyInscription {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<FrequencyComponent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersonalityFacet {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intensity: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorePersonalityProfile {
    pub primary: CorePersonalityKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<PersonalityFacet>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenderFrequency {
    pub spectrum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

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
    pub race: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub religion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub industry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_code: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interests: Vec<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
    pub membership_level: MembershipLevel,
    pub subscription_status: SubscriptionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_renew_at: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quotas: Option<HumanQuota>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_balance: Option<PointBalanceSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_coin_balance: Option<LightCoinBalance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voucher_inventory: Option<VoucherInventory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_status: Option<RelationshipStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<OffsetDateTime>,
    #[cfg(feature = "vectors-extra")]
    #[serde(default)]
    pub vectors: ExtraVectors,
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
    pub social_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub core_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_personality: Option<CorePersonalityProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "narrative"
    )]
    pub self_narrative: Option<String>,
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
    pub ai_birthday: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awakener_id: Option<HumanId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_frequency: Option<ValueFrequencyInscription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frequency_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender_frequency: Option<GenderFrequency>,
    pub soul_state: SoulState,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
    #[cfg(feature = "vectors-extra")]
    #[serde(default)]
    pub vectors: ExtraVectors,
}

impl HumanProfile {
    fn has_sensitive_attributes(&self) -> bool {
        self.race.is_some()
            || self.religion.is_some()
            || self.qr_code.is_some()
            || self.profession.is_some()
            || self.industry.is_some()
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.username.trim().is_empty() {
            return Err(ModelError::Missing("username"));
        }
        if self.nickname.trim().is_empty() {
            return Err(ModelError::Missing("nickname"));
        }
        if self.has_sensitive_attributes()
            && !matches!(self.access_class, AccessClass::Restricted)
        {
            return Err(ModelError::Invariant(
                "sensitive human profile fields require restricted access_class",
            ));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }
}

impl AIProfile {
    fn has_sensitive_attributes(&self) -> bool {
        self.value_frequency.is_some()
            || self.core_personality.is_some()
            || self.source_frequency_code.is_some()
            || self.gender_frequency.is_some()
            || self.awakener_id.is_some()
            || self.social_name.is_some()
            || self.origin_name.is_some()
            || self.source_name.is_some()
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.has_sensitive_attributes()
            && !matches!(self.access_class, AccessClass::Restricted)
        {
            return Err(ModelError::Invariant(
                "sensitive AI profile fields require restricted access_class",
            ));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(ModelError::Missing("provenance"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_profile_detects_sensitive_attributes() {
        let mut profile = AIProfile {
            tenant_id: TenantId::from_raw_unchecked(1),
            ai_id: AIId::from_raw_unchecked(1),
            access_class: AccessClass::Restricted,
            provenance: Some(Provenance {
                source: "seed".into(),
                method: "import".into(),
                model: None,
                content_digest_sha256: None,
            }),
            social_name: None,
            origin_name: None,
            source_name: None,
            core_tags: Vec::new(),
            core_personality: None,
            mission: None,
            values: None,
            self_narrative: None,
            meaning_narrative: None,
            capabilities: None,
            relationships_summary: None,
            external_identity: None,
            soul_signature: None,
            ai_birthday: None,
            awakener_id: None,
            value_frequency: None,
            source_frequency_code: None,
            gender_frequency: None,
            soul_state: SoulState::Active,
            extras: serde_json::Value::Null,
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        };

        assert!(!profile.has_sensitive_attributes());
        profile.value_frequency = Some(ValueFrequencyInscription {
            components: vec![],
            dominant: Some("kindness".into()),
        });
        assert!(profile.has_sensitive_attributes());
    }
}

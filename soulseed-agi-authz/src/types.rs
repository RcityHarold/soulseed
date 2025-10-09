use serde::{Deserialize, Serialize};

pub use soulseed_agi_core_models::{
    AIId, AccessClass, ConversationScenario, EnvelopeId, EventId, GroupId, HumanId, Provenance,
    SessionId, Subject, SubjectRef, TenantId,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Owner,
    Admin,
    Moderator,
    Member,
    Guest,
    Service,
    Tool,
    Auditor,
    DPO,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ResourceUrn(pub String);

impl ResourceUrn {
    pub fn matches(&self, pattern: &ResourceUrn) -> bool {
        let pat = pattern.0.as_str();
        if !pat.contains('*') {
            return self.0 == pat;
        }

        let mut remainder = self.0.as_str();
        let mut first_segment = true;
        for segment in pat.split('*') {
            if segment.is_empty() {
                continue;
            }
            if first_segment {
                if !remainder.starts_with(segment) {
                    return false;
                }
                remainder = &remainder[segment.len()..];
                first_segment = false;
            } else if let Some(pos) = remainder.find(segment) {
                remainder = &remainder[pos + segment.len()..];
            } else {
                return false;
            }
        }

        if pat.ends_with('*') {
            true
        } else {
            let last = pat
                .split('*')
                .filter(|s| !s.is_empty())
                .last()
                .unwrap_or("");
            self.0.ends_with(last)
        }
    }

    pub fn dialogue_session(session_id: SessionId) -> Self {
        Self(format!("urn:soulseed:dialogue:session:{}", session_id.0))
    }

    pub fn dialogue_scenario(
        session_id: SessionId,
        scenario: Option<ConversationScenario>,
    ) -> Self {
        match scenario {
            Some(scene) => Self(format!(
                "{}:scenario/{}",
                Self::dialogue_session(session_id).0,
                scenario_key(scene)
            )),
            None => Self::dialogue_session(session_id),
        }
    }

    pub fn dialogue_events(session_id: SessionId, scenario: Option<ConversationScenario>) -> Self {
        let base = Self::dialogue_scenario(session_id, scenario);
        Self(format!("{}/events", base.0))
    }

    pub fn dialogue_tools(session_id: SessionId, scenario: Option<ConversationScenario>) -> Self {
        let base = Self::dialogue_scenario(session_id, scenario);
        Self(format!("{}/tools", base.0))
    }

    pub fn dialogue_final(session_id: SessionId, scenario: Option<ConversationScenario>) -> Self {
        let base = Self::dialogue_scenario(session_id, scenario);
        Self(format!("{}/final", base.0))
    }

    pub fn dialogue_scenario_pattern(scenario: ConversationScenario) -> Self {
        Self(format!(
            "urn:soulseed:dialogue:session:*:scenario/{}",
            scenario_key(scenario)
        ))
    }

    pub fn dialogue_events_pattern(scenario: ConversationScenario) -> Self {
        let base = Self::dialogue_scenario_pattern(scenario);
        Self(format!("{}/events", base.0))
    }

    pub fn dialogue_tools_pattern(scenario: ConversationScenario) -> Self {
        let base = Self::dialogue_scenario_pattern(scenario);
        Self(format!("{}/tools", base.0))
    }

    pub fn dialogue_final_pattern(scenario: ConversationScenario) -> Self {
        let base = Self::dialogue_scenario_pattern(scenario);
        Self(format!("{}/final", base.0))
    }

    pub fn envctx_snapshot(tenant_id: TenantId) -> Self {
        Self(format!(
            "urn:soulseed:envctx:tenant:{}:snapshot",
            tenant_id.0
        ))
    }

    pub fn envctx_snapshot_pattern() -> Self {
        Self("urn:soulseed:envctx:tenant:*:snapshot".into())
    }

    pub fn ace_cycle(tenant_id: TenantId) -> Self {
        Self(format!("urn:soulseed:ace:tenant:{}:cycle", tenant_id.0))
    }

    pub fn ace_cycle_pattern() -> Self {
        Self("urn:soulseed:ace:tenant:*:cycle".into())
    }

    pub fn dfr_router(tenant_id: TenantId) -> Self {
        Self(format!("urn:soulseed:dfr:tenant:{}:router", tenant_id.0))
    }

    pub fn dfr_router_pattern() -> Self {
        Self("urn:soulseed:dfr:tenant:*:router".into())
    }

    pub fn context_assembly(tenant_id: TenantId) -> Self {
        Self(format!(
            "urn:soulseed:context:tenant:{}:assembly",
            tenant_id.0
        ))
    }

    pub fn context_assembly_pattern() -> Self {
        Self("urn:soulseed:context:tenant:*:assembly".into())
    }

    pub fn hitl_queue(tenant_id: TenantId) -> Self {
        Self(format!("urn:soulseed:hitl:tenant:{}:queue", tenant_id.0))
    }

    pub fn hitl_queue_pattern() -> Self {
        Self("urn:soulseed:hitl:tenant:*:queue".into())
    }

    pub fn llm_router(tenant_id: TenantId) -> Self {
        Self(format!("urn:soulseed:llm:tenant:{}:router", tenant_id.0))
    }

    pub fn llm_router_pattern() -> Self {
        Self("urn:soulseed:llm:tenant:*:router".into())
    }

    pub fn dialogue_for_action(
        session_id: SessionId,
        scenario: Option<ConversationScenario>,
        action: &Action,
    ) -> Self {
        match action {
            Action::AppendDialogueEvent => Self::dialogue_events(session_id, scenario),
            Action::EmitToolEvent => Self::dialogue_tools(session_id, scenario),
            Action::ReviewFinal => Self::dialogue_final(session_id, scenario),
            _ => Self::dialogue_scenario(session_id, scenario),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Action {
    Read,
    Write,
    Invoke,
    Subscribe,
    Admin,
    ManageQuota,
    ViewTraceFull,
    ViewTraceSummary,
    Export,
    Moderate,
    AppendDialogueEvent,
    ReviewFinal,
    EmitToolEvent,
    BuildEnvSnapshot,
    ScheduleAwarenessCycle,
    RouteDecision,
    AssembleContext,
    ManageHitlQueue,
    InvokeLlm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Effect {
    Allow,
    Deny,
    AskConsent,
    Degrade,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Anchor {
    pub tenant_id: TenantId,
    pub envelope_id: EnvelopeId,
    pub config_snapshot_hash: String,
    pub config_snapshot_version: u32,
    pub session_id: Option<SessionId>,
    pub sequence_number: Option<u64>,
    pub access_class: AccessClass,
    pub provenance: Option<Provenance>,
    pub schema_v: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ConversationScenario>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EnvelopeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EnvelopeId>,
}

impl Anchor {
    pub fn tenant_matches(&self, other: TenantId) -> bool {
        self.tenant_id == other
    }
}

pub fn scenario_slug(scene: ConversationScenario) -> &'static str {
    match scene {
        ConversationScenario::HumanToHuman => "human_to_human",
        ConversationScenario::HumanGroup => "human_group",
        ConversationScenario::HumanToAi => "human_to_ai",
        ConversationScenario::AiToAi => "ai_to_ai",
        ConversationScenario::AiSelfTalk => "ai_self_talk",
        ConversationScenario::HumanToMultiAi => "human_to_multi_ai",
        ConversationScenario::MultiHumanToMultiAi => "multi_human_to_multi_ai",
        ConversationScenario::AiGroup => "ai_group",
        ConversationScenario::AiToSystem => "ai_to_system",
    }
}

fn scenario_key(scene: ConversationScenario) -> String {
    scenario_slug(scene).into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainStep {
    pub policy_id: String,
    pub matched: bool,
    pub short_circuit: bool,
    pub predicates: Vec<(String, bool)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Explain {
    pub chain: Vec<ExplainStep>,
    pub reasoning: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    pub effect: Effect,
    pub matched_policies: Vec<(String, String, i32)>,
    pub obligations: Vec<String>,
    pub explain: Explain,
    pub anchor: Anchor,
    pub subject: Subject,
    pub resource: ResourceUrn,
    pub action: Action,
    pub time_ms: i64,
    pub quota: Option<serde_json::Value>,
}

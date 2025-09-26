use std::sync::Arc;

use serde_json::json;
use time::OffsetDateTime;

use crate::{
    cache::InMemoryPolicyStore,
    policy::Policy,
    policy::Predicate,
    quota::ScenarioQuotaClient,
    types::{scenario_slug, Action, ConversationScenario, Effect, ResourceUrn, Role, TenantId},
    Evaluator,
};

pub struct InMemoryEnv {
    pub policies: Arc<InMemoryPolicyStore>,
    pub quota: Arc<ScenarioQuotaClient>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl Default for InMemoryEnv {
    fn default() -> Self {
        Self {
            policies: Arc::new(InMemoryPolicyStore::default()),
            quota: Arc::new(ScenarioQuotaClient::new()),
            clock: Arc::new(|| OffsetDateTime::now_utc().unix_timestamp() * 1000),
        }
    }
}

impl InMemoryEnv {
    pub fn push_policy(&self, tenant: TenantId, policy: Policy) {
        self.policies.push(tenant, policy);
    }

    pub fn evaluator(&self) -> Evaluator {
        Evaluator::new(self.policies.clone(), self.quota.clone()).with_clock(self.clock.clone())
    }

    pub fn set_fixed_time(&mut self, time_ms: i64) {
        self.clock = Arc::new(move || time_ms);
    }

    pub fn seed_dialogue_policies(&self, tenant: TenantId) {
        use ConversationScenario::*;
        let scenarios = [
            HumanToHuman,
            HumanGroup,
            HumanToAi,
            AiToAi,
            AiSelfTalk,
            HumanToMultiAi,
            MultiHumanToMultiAi,
            AiGroup,
            AiToSystem,
        ];

        for scene in scenarios.iter().cloned() {
            let slug = scenario_slug(scene.clone());
            let scene_predicate = Predicate::SceneIn(vec![slug.to_string()]);

            self.push_policy(
                tenant,
                Policy {
                    id: format!("allow.dialogue.append.{slug}"),
                    schema_v: 1,
                    version: 1,
                    parent: None,
                    priority: 40,
                    subject_roles: vec![Role::Member, Role::Service],
                    subject_attrs: json!({}),
                    resource_urn: ResourceUrn::dialogue_events_pattern(scene.clone()),
                    actions: vec![Action::AppendDialogueEvent],
                    effect: Effect::Allow,
                    conditions: vec![Predicate::TenantEq, scene_predicate.clone()],
                    obligations: vec![],
                    policy_digest: format!("sha256:append:{slug}"),
                },
            );

            self.push_policy(
                tenant,
                Policy {
                    id: format!("allow.dialogue.tool.{slug}"),
                    schema_v: 1,
                    version: 1,
                    parent: None,
                    priority: 35,
                    subject_roles: vec![Role::Tool, Role::Service],
                    subject_attrs: json!({}),
                    resource_urn: ResourceUrn::dialogue_tools_pattern(scene.clone()),
                    actions: vec![Action::EmitToolEvent],
                    effect: Effect::Allow,
                    conditions: vec![Predicate::TenantEq, scene_predicate.clone()],
                    obligations: vec![],
                    policy_digest: format!("sha256:tool:{slug}"),
                },
            );

            self.push_policy(
                tenant,
                Policy {
                    id: format!("allow.dialogue.final.{slug}"),
                    schema_v: 1,
                    version: 1,
                    parent: None,
                    priority: 30,
                    subject_roles: vec![Role::Auditor, Role::Moderator],
                    subject_attrs: json!({}),
                    resource_urn: ResourceUrn::dialogue_final_pattern(scene.clone()),
                    actions: vec![Action::ReviewFinal],
                    effect: Effect::Allow,
                    conditions: vec![Predicate::TenantEq, scene_predicate.clone()],
                    obligations: vec![],
                    policy_digest: format!("sha256:final:{slug}"),
                },
            );

            self.push_policy(
                tenant,
                Policy {
                    id: format!("allow.dialogue.hitl.{slug}"),
                    schema_v: 1,
                    version: 1,
                    parent: None,
                    priority: 45,
                    subject_roles: vec![Role::Moderator],
                    subject_attrs: json!({}),
                    resource_urn: ResourceUrn::dialogue_events_pattern(scene.clone()),
                    actions: vec![Action::AppendDialogueEvent],
                    effect: Effect::Allow,
                    conditions: vec![
                        Predicate::TenantEq,
                        scene_predicate.clone(),
                        Predicate::HasConsent {
                            kind: "hitl".into(),
                        },
                    ],
                    obligations: vec![],
                    policy_digest: format!("sha256:hitl:{slug}"),
                },
            );
        }
    }

    pub fn seed_infra_policies(&self, tenant: TenantId) {
        let base_conditions = vec![Predicate::TenantEq];

        self.push_policy(
            tenant,
            Policy {
                id: "allow.envctx.build".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 60,
                subject_roles: vec![Role::Service],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::envctx_snapshot_pattern(),
                actions: vec![Action::BuildEnvSnapshot],
                effect: Effect::Allow,
                conditions: base_conditions.clone(),
                obligations: vec![],
                policy_digest: "sha256:envctx".into(),
            },
        );

        self.push_policy(
            tenant,
            Policy {
                id: "allow.ace.schedule".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 58,
                subject_roles: vec![Role::Service],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::ace_cycle_pattern(),
                actions: vec![Action::ScheduleAwarenessCycle],
                effect: Effect::Allow,
                conditions: base_conditions.clone(),
                obligations: vec![],
                policy_digest: "sha256:ace".into(),
            },
        );

        self.push_policy(
            tenant,
            Policy {
                id: "allow.dfr.route".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 57,
                subject_roles: vec![Role::Service],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::dfr_router_pattern(),
                actions: vec![Action::RouteDecision],
                effect: Effect::Allow,
                conditions: base_conditions.clone(),
                obligations: vec![],
                policy_digest: "sha256:dfr".into(),
            },
        );

        self.push_policy(
            tenant,
            Policy {
                id: "allow.context.assembly".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 55,
                subject_roles: vec![Role::Service],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::context_assembly_pattern(),
                actions: vec![Action::AssembleContext],
                effect: Effect::Allow,
                conditions: base_conditions.clone(),
                obligations: vec![],
                policy_digest: "sha256:context".into(),
            },
        );

        self.push_policy(
            tenant,
            Policy {
                id: "allow.hitl.manage".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 54,
                subject_roles: vec![Role::Moderator],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::hitl_queue_pattern(),
                actions: vec![Action::ManageHitlQueue],
                effect: Effect::Allow,
                conditions: base_conditions.clone(),
                obligations: vec![],
                policy_digest: "sha256:hitl".into(),
            },
        );

        self.push_policy(
            tenant,
            Policy {
                id: "allow.llm.invoke".into(),
                schema_v: 1,
                version: 1,
                parent: None,
                priority: 59,
                subject_roles: vec![Role::Service, Role::Tool],
                subject_attrs: json!({}),
                resource_urn: ResourceUrn::llm_router_pattern(),
                actions: vec![Action::InvokeLlm],
                effect: Effect::Allow,
                conditions: base_conditions,
                obligations: vec![],
                policy_digest: "sha256:llm".into(),
            },
        );
    }
}

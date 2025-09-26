use serde_json::json;
use soulseed_agi_authz::api::AuthzRequest;
use soulseed_agi_authz::errors::AuthzError;
use soulseed_agi_authz::mock::InMemoryEnv;
use soulseed_agi_authz::policy::{Obligation, Policy, Predicate};
use soulseed_agi_authz::quota::QuotaCost;
use soulseed_agi_authz::types::{
    AccessClass, Action, Anchor, ConversationScenario, Effect, HumanId, Provenance, ResourceUrn,
    Role, SessionId, Subject, TenantId,
};
use uuid::Uuid;

fn scenario_slug(scene: ConversationScenario) -> &'static str {
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

fn default_anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(7)),
        sequence_number: Some(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "ui".into(),
            method: "test".into(),
            model: None,
            content_digest_sha256: None,
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
    }
}

fn anchor_scene(anchor: &Anchor) -> ConversationScenario {
    anchor
        .scenario
        .clone()
        .expect("anchor must carry scenario for dialogue policies")
}

fn anchor_scene_slug(anchor: &Anchor) -> &'static str {
    scenario_slug(anchor_scene(anchor))
}

#[test]
fn default_deny_when_no_policy_matches() {
    let env = InMemoryEnv::default();
    let req = AuthzRequest {
        anchor: default_anchor(),
        subject: Subject::Human(HumanId(9)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_session(SessionId(7)),
        action: Action::Read,
        context: json!({"scene": scenario_slug(ConversationScenario::HumanToAi)}),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: None,
        idem_key: None,
    };
    let resp = env.evaluator().authorize(req).unwrap();
    assert_eq!(resp.decision.effect, Effect::Deny);
    assert!(resp.decision.matched_policies.is_empty());
}

#[test]
fn deny_policy_short_circuit() {
    let env = InMemoryEnv::default();
    env.push_policy(
        TenantId(1),
        Policy {
            id: "deny.read".into(),
            schema_v: 1,
            version: 1,
            parent: None,
            priority: 100,
            subject_roles: vec![Role::Member],
            subject_attrs: json!({}),
            resource_urn: ResourceUrn("urn:soulseed:dialogue:session:*".into()),
            actions: vec![Action::Read],
            effect: Effect::Deny,
            conditions: vec![Predicate::TenantEq],
            obligations: vec![Obligation::Custom("reason=explicit_deny".into())],
            policy_digest: "sha256:deny".into(),
        },
    );
    let req = AuthzRequest {
        anchor: default_anchor(),
        subject: Subject::Human(HumanId(9)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_session(SessionId(7)),
        action: Action::Read,
        context: json!({
            "groups": [123],
            "scene": scenario_slug(ConversationScenario::HumanToAi)
        }),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: None,
        idem_key: None,
    };
    let resp = env.evaluator().authorize(req).unwrap();
    assert_eq!(resp.decision.effect, Effect::Deny);
    assert_eq!(resp.decision.matched_policies[0].0, "deny.read");
}

#[test]
fn restricted_requires_provenance() {
    let env = InMemoryEnv::default();
    let mut anchor = default_anchor();
    anchor.provenance = None;
    let req = AuthzRequest {
        anchor,
        subject: Subject::Human(HumanId(9)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_session(SessionId(7)),
        action: Action::Read,
        context: json!({"scene": scenario_slug(ConversationScenario::HumanToAi)}),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: None,
        idem_key: None,
    };
    let err = env.evaluator().authorize(req).unwrap_err();
    assert_eq!(err, AuthzError::PrivacyRestricted);
}

#[test]
fn view_trace_full_requires_ticket() {
    let env = InMemoryEnv::default();
    env.push_policy(
        TenantId(1),
        Policy {
            id: "allow.trace.full".into(),
            schema_v: 1,
            version: 1,
            parent: None,
            priority: 50,
            subject_roles: vec![Role::Owner],
            subject_attrs: json!({}),
            resource_urn: ResourceUrn("urn:soulseed:model:trace:*".into()),
            actions: vec![Action::ViewTraceFull],
            effect: Effect::Allow,
            conditions: vec![Predicate::TenantEq],
            obligations: vec![],
            policy_digest: "sha256:t".into(),
        },
    );
    let req = AuthzRequest {
        anchor: default_anchor(),
        subject: Subject::Human(HumanId(1001)),
        roles: vec![Role::Owner],
        resource: ResourceUrn("urn:soulseed:model:trace:evt-901".into()),
        action: Action::ViewTraceFull,
        context: json!({}),
        want_trace_full: true,
        access_ticket: None,
        quota_cost: None,
        idem_key: None,
    };
    let resp = env.evaluator().authorize(req).unwrap();
    assert_eq!(resp.decision.effect, Effect::Deny);
    assert!(resp
        .decision
        .obligations
        .iter()
        .any(|o| o.contains("TRACE_FORBIDDEN")));
}

#[test]
fn quota_idempotency_only_first_consumes() {
    let env = InMemoryEnv::default();
    env.push_policy(
        TenantId(1),
        Policy {
            id: "allow.write".into(),
            schema_v: 1,
            version: 1,
            parent: None,
            priority: 60,
            subject_roles: vec![Role::Member],
            subject_attrs: json!({}),
            resource_urn: ResourceUrn("urn:soulseed:dialogue:session:*".into()),
            actions: vec![Action::Write],
            effect: Effect::Allow,
            conditions: vec![Predicate::TenantEq],
            obligations: vec![],
            policy_digest: "sha256:w".into(),
        },
    );

    let request = |idem: &str| AuthzRequest {
        anchor: default_anchor(),
        subject: Subject::Human(HumanId(9)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_session(SessionId(7)),
        action: Action::Write,
        context: json!({"scene": scenario_slug(ConversationScenario::HumanToAi)}),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: Some(QuotaCost {
            items: vec![("chat.rounds".into(), 1)],
        }),
        idem_key: Some(idem.into()),
    };

    let eval = env.evaluator();
    let first = eval.authorize(request("idem-1")).unwrap();
    let second = eval.authorize(request("idem-1")).unwrap();
    assert_eq!(first.decision.effect, Effect::Allow);
    assert_eq!(second.decision.effect, Effect::Allow);
    assert_eq!(
        second.decision.quota.unwrap()["reason"].as_str().unwrap(),
        "idempotent"
    );
}

#[test]
fn anchor_echoes_in_decision() {
    let env = InMemoryEnv::default();
    env.push_policy(
        TenantId(1),
        Policy {
            id: "allow.read".into(),
            schema_v: 1,
            version: 1,
            parent: None,
            priority: 30,
            subject_roles: vec![Role::Member],
            subject_attrs: json!({}),
            resource_urn: ResourceUrn("urn:soulseed:dialogue:session:*".into()),
            actions: vec![Action::Read],
            effect: Effect::Allow,
            conditions: vec![Predicate::TenantEq],
            obligations: vec![],
            policy_digest: "sha256:r".into(),
        },
    );
    let anchor = default_anchor();
    let resp = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(9)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_session(SessionId(7)),
            action: Action::Read,
            context: json!({"scene": scenario_slug(ConversationScenario::HumanToAi)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(resp.decision.anchor.tenant_id.0, anchor.tenant_id.0);
    assert_eq!(
        resp.decision.anchor.config_snapshot_hash,
        anchor.config_snapshot_hash
    );
}

#[test]
fn tool_quota_budget_degrades() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let evaluator = env.evaluator();
    let anchor = default_anchor();
    let make_req = |cost: i64, idem: &str| AuthzRequest {
        anchor: anchor.clone(),
        subject: Subject::Human(HumanId(900)),
        roles: vec![Role::Tool],
        resource: ResourceUrn::dialogue_for_action(
            anchor.session_id.unwrap(),
            anchor.scenario.clone(),
            &Action::EmitToolEvent,
        ),
        action: Action::EmitToolEvent,
        context: json!({"scene": anchor_scene_slug(&anchor)}),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: Some(QuotaCost {
            items: vec![("tool.tokens".into(), cost)],
        }),
        idem_key: Some(idem.into()),
    };

    let first = evaluator.authorize(make_req(40, "tool-1")).unwrap();
    assert_eq!(first.decision.effect, Effect::Allow);
    assert_eq!(
        first.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "tool_budget_allow"
    );

    let second = evaluator.authorize(make_req(50, "tool-2")).unwrap();
    assert_eq!(second.decision.effect, Effect::Allow);

    let third = evaluator.authorize(make_req(20, "tool-3")).unwrap();
    assert_eq!(third.decision.effect, Effect::Degrade);
    assert_eq!(
        third.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "tool_budget_exceeded"
    );
}

#[test]
fn self_reflection_fast_lane_skips_quota() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let evaluator = env.evaluator();
    let mut anchor = default_anchor();
    anchor.scenario = Some(ConversationScenario::AiSelfTalk);
    let resp = evaluator
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(77)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::AppendDialogueEvent,
            ),
            action: Action::AppendDialogueEvent,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: Some(QuotaCost::default()),
            idem_key: Some("self-ref".into()),
        })
        .unwrap();
    assert_eq!(resp.decision.effect, Effect::Allow);
    assert_eq!(
        resp.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "self_reflection_fastlane"
    );
}

#[test]
fn clarify_retry_limit_triggers_degrade() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let evaluator = env.evaluator();
    let anchor = default_anchor();
    let make_req = |idem: &str| AuthzRequest {
        anchor: anchor.clone(),
        subject: Subject::Human(HumanId(55)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_for_action(
            anchor.session_id.unwrap(),
            anchor.scenario.clone(),
            &Action::AppendDialogueEvent,
        ),
        action: Action::AppendDialogueEvent,
        context: json!({
            "scene": anchor_scene_slug(&anchor),
            "stage": "clarify"
        }),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: Some(QuotaCost {
            items: vec![("chat.rounds".into(), 1)],
        }),
        idem_key: Some(idem.into()),
    };

    for idx in 0..3 {
        let resp = evaluator
            .authorize(make_req(&format!("clarify-{idx}")))
            .unwrap();
        assert_eq!(resp.decision.effect, Effect::Allow);
    }

    let fourth = evaluator.authorize(make_req("clarify-3")).unwrap();
    assert_eq!(fourth.decision.effect, Effect::Degrade);
    assert_eq!(
        fourth.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "clarify_retry_limit"
    );
}

#[test]
fn append_dialogue_event_respects_scene_policy() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let anchor = default_anchor();
    let resp = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(9)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::AppendDialogueEvent,
            ),
            action: Action::AppendDialogueEvent,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(resp.decision.effect, Effect::Allow);
}

#[test]
fn emit_tool_event_requires_tool_role() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let anchor = default_anchor();
    let denied = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(9)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::EmitToolEvent,
            ),
            action: Action::EmitToolEvent,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(denied.decision.effect, Effect::Deny);

    let allowed = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(9)),
            roles: vec![Role::Tool],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::EmitToolEvent,
            ),
            action: Action::EmitToolEvent,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(allowed.decision.effect, Effect::Allow);
}

#[test]
fn review_final_requires_auditor_role() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let anchor = default_anchor();
    let denied = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(9)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::ReviewFinal,
            ),
            action: Action::ReviewFinal,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(denied.decision.effect, Effect::Deny);

    let allowed = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(42)),
            roles: vec![Role::Auditor],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::ReviewFinal,
            ),
            action: Action::ReviewFinal,
            context: json!({"scene": anchor_scene_slug(&anchor)}),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(allowed.decision.effect, Effect::Allow);
}

#[test]
fn hitl_append_requires_consent() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let anchor = default_anchor();
    let missing = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(11)),
            roles: vec![Role::Moderator],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::AppendDialogueEvent,
            ),
            action: Action::AppendDialogueEvent,
            context: json!({
                "scene": anchor_scene_slug(&anchor)
            }),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(missing.decision.effect, Effect::Deny);

    let allowed = env
        .evaluator()
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(11)),
            roles: vec![Role::Moderator],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::AppendDialogueEvent,
            ),
            action: Action::AppendDialogueEvent,
            context: json!({
                "scene": anchor_scene_slug(&anchor),
                "consents": ["hitl"],
            }),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: None,
            idem_key: None,
        })
        .unwrap();
    assert_eq!(allowed.decision.effect, Effect::Allow);
}

#[test]
fn infra_policies_allow_expected_roles() {
    let env = InMemoryEnv::default();
    env.seed_infra_policies(TenantId(1));
    let evaluator = env.evaluator();
    let anchor = default_anchor();

    let decision = |resource: ResourceUrn, action: Action, roles: Vec<Role>| {
        evaluator
            .authorize(AuthzRequest {
                anchor: anchor.clone(),
                subject: Subject::System,
                roles,
                resource,
                action,
                context: json!({}),
                want_trace_full: false,
                access_ticket: None,
                quota_cost: None,
                idem_key: None,
            })
            .unwrap()
            .decision
            .effect
    };

    assert_eq!(
        decision(
            ResourceUrn::envctx_snapshot(anchor.tenant_id),
            Action::BuildEnvSnapshot,
            vec![Role::Service]
        ),
        Effect::Allow
    );
    assert_eq!(
        decision(
            ResourceUrn::envctx_snapshot(anchor.tenant_id),
            Action::BuildEnvSnapshot,
            vec![Role::Member]
        ),
        Effect::Deny
    );

    assert_eq!(
        decision(
            ResourceUrn::ace_cycle(anchor.tenant_id),
            Action::ScheduleAwarenessCycle,
            vec![Role::Service]
        ),
        Effect::Allow
    );

    assert_eq!(
        decision(
            ResourceUrn::dfr_router(anchor.tenant_id),
            Action::RouteDecision,
            vec![Role::Service]
        ),
        Effect::Allow
    );

    assert_eq!(
        decision(
            ResourceUrn::context_assembly(anchor.tenant_id),
            Action::AssembleContext,
            vec![Role::Service]
        ),
        Effect::Allow
    );

    assert_eq!(
        decision(
            ResourceUrn::hitl_queue(anchor.tenant_id),
            Action::ManageHitlQueue,
            vec![Role::Moderator]
        ),
        Effect::Allow
    );
    assert_eq!(
        decision(
            ResourceUrn::hitl_queue(anchor.tenant_id),
            Action::ManageHitlQueue,
            vec![Role::Member]
        ),
        Effect::Deny
    );

    assert_eq!(
        decision(
            ResourceUrn::llm_router(anchor.tenant_id),
            Action::InvokeLlm,
            vec![Role::Tool]
        ),
        Effect::Allow
    );
    assert_eq!(
        decision(
            ResourceUrn::llm_router(anchor.tenant_id),
            Action::InvokeLlm,
            vec![Role::Guest]
        ),
        Effect::Deny
    );
}

#[test]
fn clarify_concurrency_limit_degrades() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let evaluator = env.evaluator();
    let anchor = default_anchor();
    let resp = evaluator
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(55)),
            roles: vec![Role::Member],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::AppendDialogueEvent,
            ),
            action: Action::AppendDialogueEvent,
            context: json!({
                "scene": anchor_scene_slug(&anchor),
                "stage": "clarify",
                "clarify_pending": 2
            }),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: Some(QuotaCost {
                items: vec![("chat.rounds".into(), 1)],
            }),
            idem_key: Some("clarify-concurrency".into()),
        })
        .unwrap();
    assert_eq!(resp.decision.effect, Effect::Degrade);
    assert_eq!(
        resp.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "clarify_concurrency_limit"
    );
}

#[test]
fn collab_size_limit_triggers_degrade() {
    let env = InMemoryEnv::default();
    env.seed_dialogue_policies(TenantId(1));
    let evaluator = env.evaluator();
    let anchor = default_anchor();

    let resp = evaluator
        .authorize(AuthzRequest {
            anchor: anchor.clone(),
            subject: Subject::Human(HumanId(88)),
            roles: vec![Role::Tool],
            resource: ResourceUrn::dialogue_for_action(
                anchor.session_id.unwrap(),
                anchor.scenario.clone(),
                &Action::EmitToolEvent,
            ),
            action: Action::EmitToolEvent,
            context: json!({
                "scene": anchor_scene_slug(&anchor),
                "collab_participants": 12
            }),
            want_trace_full: false,
            access_ticket: None,
            quota_cost: Some(QuotaCost {
                items: vec![("tool.tokens".into(), 5)],
            }),
            idem_key: Some("collab-limit".into()),
        })
        .unwrap();
    assert_eq!(resp.decision.effect, Effect::Degrade);
    assert_eq!(
        resp.decision.quota.as_ref().unwrap()["reason"]
            .as_str()
            .unwrap(),
        "collab_size_exceeded"
    );
}

use serde_json::json;
use soulseed_agi_authz::api::AuthzRequest;
use soulseed_agi_authz::mock::InMemoryEnv;
use soulseed_agi_authz::policy::{Obligation, Policy, Predicate};
use soulseed_agi_authz::quota::QuotaCost;
use soulseed_agi_authz::types::{
    scenario_slug, Action, Anchor, ConversationScenario, Effect, HumanId, Provenance, ResourceUrn,
    Role, SessionId, Subject, TenantId,
};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = InMemoryEnv::default();
    env.push_policy(
        TenantId(1),
        Policy {
            id: "allow.session.read".into(),
            schema_v: 1,
            version: 1,
            parent: None,
            priority: 10,
            subject_roles: vec![Role::Member],
            subject_attrs: json!({}),
            resource_urn: ResourceUrn("urn:soulseed:dialogue:session:*".into()),
            actions: vec![Action::Read],
            effect: Effect::Allow,
            conditions: vec![Predicate::TenantEq],
            obligations: vec![Obligation::LogExplain],
            policy_digest: "sha256:allow-session-read".into(),
        },
    );

    let anchor = Anchor {
        tenant_id: TenantId(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(42)),
        sequence_number: Some(1),
        access_class: soulseed_agi_authz::types::AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "ui".into(),
            method: "read".into(),
            model: None,
            content_digest_sha256: None,
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
    };

    let request = AuthzRequest {
        anchor: anchor.clone(),
        subject: Subject::Human(HumanId(1001)),
        roles: vec![Role::Member],
        resource: ResourceUrn::dialogue_session(SessionId(42)),
        action: Action::Read,
        context: json!({"scene": scenario_slug(ConversationScenario::HumanToAi)}),
        want_trace_full: false,
        access_ticket: None,
        quota_cost: Some(QuotaCost {
            items: vec![("chat.rounds".into(), 1)],
        }),
        idem_key: Some("idem-example".into()),
    };

    let evaluator = env.evaluator();
    let response = evaluator.authorize(request)?;
    println!("decision: {:?}", response.decision.effect);
    println!("obligations: {:?}", response.decision.obligations);
    println!("explain: {:?}", response.decision.explain.reasoning);
    Ok(())
}

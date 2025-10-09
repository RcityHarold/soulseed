use once_cell::sync::Lazy;
use serde_json::json;
use soulseed_agi_tools::config::ToolsConfig;
use soulseed_agi_tools::dto::{
    AccessClass, Anchor, ConversationScenario, DialogueEventType, EngineInput, Provenance,
    SessionId, TenantId, ToolLane,
};
use soulseed_agi_tools::engine::ToolEngine;
use soulseed_agi_tools::errors::EngineError;
use soulseed_agi_tools::explainer::DefaultExplainer;
use soulseed_agi_tools::metrics::NoopMetrics;
use soulseed_agi_tools::orchestrator::ToolOrchestrator;
use soulseed_agi_tools::planner::ToolPlanner;
use soulseed_agi_tools::router::RouterDeterministic;
use soulseed_agi_tools::traits::ToolEngineRunner;
use soulseed_agi_tools::tw_client::{ToolDef, TwClientMock};
use uuid::Uuid;

static TEST_ANCHOR: Lazy<Anchor> = Lazy::new(|| Anchor {
    tenant_id: TenantId(1),
    envelope_id: Uuid::now_v7(),
    config_snapshot_hash: "cfg-tools-default".into(),
    config_snapshot_version: 1,
    session_id: Some(SessionId(9)),
    sequence_number: Some(1),
    access_class: AccessClass::Restricted,
    provenance: Some(Provenance {
        source: "router".into(),
        method: "unit_test".into(),
        model: None,
        content_digest_sha256: Some("sha256:demo".into()),
    }),
    schema_v: 1,
    scenario: None,
    supersedes: None,
    superseded_by: None,
});

fn default_engine<'a>(config: ToolsConfig) -> ToolEngine<'a> {
    ToolEngine {
        router: &RouterDeterministic,
        planner: &ToolPlanner,
        orchestrator: &ToolOrchestrator,
        explainer: &DefaultExplainer,
        config,
    }
}

fn def_tool(tool_id: &str, capability: &[&str], risk: &str) -> ToolDef {
    ToolDef {
        tool_id: tool_id.into(),
        version: "1".into(),
        capability: capability.iter().map(|s| s.to_string()).collect(),
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        side_effect: risk.eq_ignore_ascii_case("high"),
        supports_stream: !risk.eq_ignore_ascii_case("high"),
        risk_level: risk.into(),
    }
}

fn base_input(anchor: Anchor) -> EngineInput {
    EngineInput {
        anchor,
        scene: "search".into(),
        capability_hints: vec!["search".into()],
        context_tags: json!({
            "stats": {
                "web.search": {
                    "success_rate": 0.9,
                    "latency_p95": 0.3,
                    "cost_per_success": 0.6,
                    "risk_level": "low",
                    "degradation_acceptance": 0.6
                }
            }
        }),
        request_context: json!({
            "inputs": {
                "web.search": { "query": "soulseed" }
            }
        }),
        subject: None,
    }
}

#[test]
fn plan_carries_version_and_lineage() {
    let anchor = TEST_ANCHOR.clone();
    let config = ToolsConfig::default();
    let engine = default_engine(config.clone());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], "low")]);
    let obs = NoopMetrics;

    let output = engine
        .run(base_input(anchor.clone()), &tw, &obs)
        .expect("plan build");

    assert_eq!(output.plan.schema_v, anchor.schema_v);
    assert_eq!(
        output.plan.lineage.version,
        anchor.sequence_number.unwrap() as u32
    );
    assert_eq!(output.plan.explain_plan.schema_v, anchor.schema_v);
    assert!(output.execution.summary.is_none());
    assert_eq!(
        output.execution.meta.per_tool_degradation.len(),
        config.budget.degrade_rules.len()
    );
    assert_eq!(output.dialogue_events.len(), output.plan.items.len());
    assert!(output
        .dialogue_events
        .iter()
        .all(|event| event.event_type == DialogueEventType::ToolCall));
}

#[test]
fn router_exclusions_surface_in_explain_plan() {
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 1;
    config.router.risk_threshold = "low".into();
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![
        def_tool("web.search", &["search"], "low"),
        def_tool("danger.ops", &["search"], "critical"),
    ]);
    let obs = NoopMetrics;

    let mut input = base_input(TEST_ANCHOR.clone());
    input.context_tags["stats"]["danger.ops"] = json!({
        "success_rate": 0.95,
        "latency_p95": 0.4,
        "cost_per_success": 0.5,
        "risk_level": "critical"
    });

    let output = engine.run(input, &tw, &obs).expect("plan with exclusions");
    assert_eq!(output.plan.items.len(), 1);
    assert!(output
        .plan
        .explain_plan
        .excluded
        .iter()
        .any(|(tool, reason)| tool == "danger.ops" && reason == "risk_threshold"));
}

#[test]
fn restricted_without_provenance_rejected() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.provenance = None;
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], "low")]);
    let obs = NoopMetrics;
    let input = base_input(anchor);

    let result = engine.run(input, &tw, &obs);
    assert!(matches!(result.unwrap_err(), EngineError::Tool(_)));
}

#[test]
fn self_reflection_lane_adjusts_strategy() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.scenario = Some(ConversationScenario::AiSelfTalk);
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![def_tool("reflector", &["self_reflection"], "very_low")]);
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.anchor = anchor;
    input.scene = "self_reflection".into();
    input.capability_hints = vec!["self_reflection".into()];
    input.context_tags["stats"]["reflector"] = json!({
        "success_rate": 0.9,
        "latency_p95": 0.2,
        "cost_per_success": 0.3,
        "risk_level": "very_low"
    });
    input.request_context["inputs"]["reflector"] = json!({"prompt": "analyze"});

    let output = engine.run(input, &tw, &obs).expect("self reflection plan");
    assert_eq!(output.plan.lane, ToolLane::SelfReflection);
    assert_eq!(output.plan.strategy.mode, soulseed_agi_tools::dto::OrchestrationMode::Serial);
    assert!(output.plan.items.len() >= 1);
    assert_eq!(output.dialogue_events.len(), output.plan.items.len());
}

#[test]
fn collaboration_lane_expands_budget() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.scenario = Some(ConversationScenario::AiToAi);
    let config = ToolsConfig::default();
    let base_cost = config.to_budget().max_cost_tokens;
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![def_tool("collab.alpha", &["collaboration"], "medium")]);
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.anchor = anchor;
    input.scene = "collaboration".into();
    input.capability_hints = vec!["collaboration".into()];
    input.context_tags["stats"]["collab.alpha"] = json!({
        "success_rate": 0.8,
        "latency_p95": 0.6,
        "cost_per_success": 0.7,
        "risk_level": "medium"
    });
    input.request_context["inputs"]["collab.alpha"] = json!({"task": "brainstorm"});

    let output = engine.run(input, &tw, &obs).expect("collaboration plan");
    assert_eq!(output.plan.lane, ToolLane::Collaboration);
    assert!(output.plan.budget.max_cost_tokens > base_cost);
}

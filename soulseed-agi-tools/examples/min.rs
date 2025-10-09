use serde_json::json;
use soulseed_agi_tools::config::ToolsConfig;
use soulseed_agi_tools::dto::{
    AccessClass, Anchor, ConversationScenario, EngineInput, Provenance, SessionId, TenantId,
};
use soulseed_agi_tools::engine::ToolEngine;
use soulseed_agi_tools::explainer::DefaultExplainer;
use soulseed_agi_tools::metrics::NoopMetrics;
use soulseed_agi_tools::orchestrator::ToolOrchestrator;
use soulseed_agi_tools::planner::ToolPlanner;
use soulseed_agi_tools::router::RouterDeterministic;
use soulseed_agi_tools::traits::ToolEngineRunner;
use soulseed_agi_tools::tw_client::{ToolDef, TwClientMock};
use uuid::Uuid;

fn make_anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId(7),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg-tools-default".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(42)),
        sequence_number: Some(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "example".into(),
            method: "demo".into(),
            model: None,
            content_digest_sha256: Some("sha256:demo".into()),
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anchor = make_anchor();
    let config = ToolsConfig::default();
    let engine = ToolEngine {
        router: &RouterDeterministic,
        planner: &ToolPlanner,
        orchestrator: &ToolOrchestrator,
        explainer: &DefaultExplainer,
        config,
    };

    let tw = TwClientMock::with_defs(vec![ToolDef {
        tool_id: "web.search".into(),
        version: "1".into(),
        capability: vec!["search".into()],
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        side_effect: false,
        supports_stream: true,
        risk_level: "medium".into(),
    }]);

    let input = EngineInput {
        anchor,
        scene: "search".into(),
        capability_hints: vec!["search".into()],
        context_tags: json!({"stats": {"web.search": {"success_rate": 0.9, "latency_p95": 0.2}}}),
        request_context: json!({"inputs": {"web.search": {"query": "Soulseed"}}}),
        subject: None,
    };
    let obs = NoopMetrics;

    let out = engine.run(input, &tw, &obs)?;
    println!("plan_id: {}", out.plan.plan_id);
    println!(
        "tools: {:?}",
        out.plan
            .items
            .iter()
            .map(|s| &s.tool_id)
            .collect::<Vec<_>>()
    );
    println!("fallback: {}", out.execution.fallback_triggered);
    println!("explain stages: {}", out.execution.explain_run.stages.len());
    println!("dialogue events: {}", out.dialogue_events.len());
    Ok(())
}

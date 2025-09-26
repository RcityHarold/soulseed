use once_cell::sync::Lazy;
use serde_json::json;
use soulseed_agi_tools::config::ToolsConfig;
use soulseed_agi_tools::dto::{
    AccessClass, Anchor, ConversationScenario, DialogueEventType, EngineInput, EvidencePointer,
    OrchestrationMode, Provenance, SessionId, TenantId, ToolEdgeKind, ToolLane, ToolResultSummary,
};
use soulseed_agi_tools::engine::ToolEngine;
use soulseed_agi_tools::errors::EngineError;
use soulseed_agi_tools::explainer::DefaultExplainer;
use soulseed_agi_tools::metrics::NoopMetrics;
use soulseed_agi_tools::orchestrator::ToolOrchestrator;
use soulseed_agi_tools::planner::ToolPlanner;
use soulseed_agi_tools::router::RouterDeterministic;
use soulseed_agi_tools::traits::ToolEngineRunner;
use soulseed_agi_tools::tw_client::{ToolDef, TwClientMock, TwError, TwEvent, TwExecuteResult};
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

fn def_tool(tool_id: &str, capability: &[&str], side_effect: bool) -> ToolDef {
    ToolDef {
        tool_id: tool_id.into(),
        version: "1".into(),
        capability: capability.iter().map(|s| s.to_string()).collect(),
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        side_effect,
        supports_stream: !side_effect,
        risk_level: if side_effect { "high" } else { "medium" }.into(),
    }
}

fn base_input(anchor: Anchor) -> EngineInput {
    EngineInput {
        anchor,
        scene: "search".into(),
        capability_hints: vec!["search".into()],
        context_tags: json!({
            "stats": {
                "web.search": { "success_rate": 0.9, "latency_p95": 0.3, "cost_per_success": 0.6, "risk_level": "low" },
                "web.cache": { "success_rate": 0.7, "latency_p95": 0.1, "cost_per_success": 0.3, "risk_level": "very_low", "cacheable": true }
            }
        }),
        request_context: json!({
            "inputs": {
                "web.search": { "query": "soulseed" },
                "web.cache": { "query": "soulseed" }
            }
        }),
        subject: None,
    }
}

#[test]
fn tools_list_compliance() {
    let anchor = TEST_ANCHOR.clone();
    let config = ToolsConfig::default();
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], false)]);
    let obs = NoopMetrics;

    let output = engine
        .run(base_input(anchor.clone()), &tw, &obs)
        .expect("plan build");
    assert_eq!(output.plan.anchor.tenant_id, anchor.tenant_id);
    assert!(output
        .plan
        .items
        .iter()
        .all(|spec| spec.tool_id == "web.search"));
    assert!(output.plan.plan_id.starts_with("plan-"));
    assert_eq!(output.dialogue_events.len(), 2);
    assert_eq!(
        output.dialogue_events[0].event_type,
        DialogueEventType::ToolCall
    );
    assert_eq!(
        output.dialogue_events[1].event_type,
        DialogueEventType::ToolResult
    );
}

#[test]
fn deterministic_plan_same_input() {
    let anchor = TEST_ANCHOR.clone();
    let config = ToolsConfig::default();
    let engine = default_engine(config.clone());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], false)]);
    let obs = NoopMetrics;
    let input = base_input(anchor.clone());

    let out1 = engine.run(input.clone(), &tw, &obs).unwrap();
    let out2 = engine.run(input, &tw, &obs).unwrap();

    assert_eq!(out1.plan.plan_id, out2.plan.plan_id);
    assert_eq!(out1.plan.items.len(), out2.plan.items.len());
    assert_eq!(out1.dialogue_events.len(), out2.dialogue_events.len());
}

#[test]
fn fallback_on_timeout_triggers_second_candidate() {
    let anchor = TEST_ANCHOR.clone();
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 2;
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![
        def_tool("web.search", &["search"], false),
        def_tool("web.cache", &["search"], false),
    ]);
    tw.push_execute(Err(TwError::Failure("TOOL.TIMEOUT".into())));
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "web.cache".into(),
            schema_v: 1,
            summary: json!({"cached": true}),
            evidence_pointer: Some(EvidencePointer {
                uri: "s3://cache".into(),
                blob_ref: None,
                span: None,
                checksum: "sha256:cache".into(),
                access_policy: "internal".into(),
            }),
            result_digest: "digest-cache".into(),
        },
        degradation_reason: Some("timeout_fallback".into()),
        indices_used: None,
        query_hash: Some("hash123".into()),
    }));

    let obs = NoopMetrics;
    let mut input = base_input(anchor.clone());
    input.request_context["inputs"]["web.cache"] = json!({"query": "soulseed"});

    let output = engine.run(input, &tw, &obs).unwrap();
    assert_eq!(output.plan.items.len(), 2);
    assert_eq!(output.execution.summary.unwrap().tool_id, "web.cache");
    assert!(output.execution.fallback_triggered);
    assert_eq!(
        output.execution.explain_run.degradation_reason.as_deref(),
        Some("timeout_fallback")
    );

    let cache_result_event = output
        .dialogue_events
        .iter()
        .find(|event| {
            matches!(event.event_type, DialogueEventType::ToolResult)
                && event.tool_result.as_ref().map(|res| res.tool_id.as_str()) == Some("web.cache")
        })
        .expect("cache result event");
    assert_eq!(
        cache_result_event.evidence_pointer.as_deref(),
        Some("s3://cache")
    );
    assert_eq!(
        cache_result_event.content_digest_sha256.as_deref(),
        Some("digest-cache")
    );
    assert_eq!(
        cache_result_event
            .tool_result
            .as_ref()
            .and_then(|res| res.degradation_reason.clone()),
        Some("timeout_fallback".into())
    );

    let events = tw.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, TwEvent::ToolFailed { tool_id, .. } if tool_id == "web.search")));
    assert!(events
        .iter()
        .any(|e| matches!(e, TwEvent::ToolResponded { tool_id, .. } if tool_id == "web.cache")));
    assert_eq!(output.dialogue_events.len(), 4);
}

#[test]
fn reconcile_called_once_on_success() {
    let anchor = TEST_ANCHOR.clone();
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], false)]);
    let obs = NoopMetrics;

    let output = engine.run(base_input(anchor.clone()), &tw, &obs).unwrap();
    assert!(output.execution.summary.is_some());
    let reconciles = tw.take_reconcile_calls();
    assert_eq!(reconciles.len(), 1);
    assert_eq!(output.dialogue_events.len(), 2);
}

#[test]
fn degradation_reason_propagates_to_explain_run() {
    let anchor = TEST_ANCHOR.clone();
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], false)]);
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "web.search".into(),
            schema_v: 1,
            summary: json!({"hits": 3}),
            evidence_pointer: None,
            result_digest: "digest-hits".into(),
        },
        degradation_reason: Some("graph_sparse_only".into()),
        indices_used: Some(vec!["idx1".into()]),
        query_hash: Some("qhash".into()),
    }));
    let obs = NoopMetrics;

    let output = engine.run(base_input(anchor.clone()), &tw, &obs).unwrap();
    assert_eq!(
        output.execution.explain_run.degradation_reason.as_deref(),
        Some("graph_sparse_only")
    );
    assert_eq!(
        output.execution.explain_run.indices_used.as_ref().unwrap()[0],
        "idx1"
    );
    assert_eq!(output.dialogue_events.len(), 2);
}

#[test]
fn restricted_without_provenance_rejected() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.provenance = None;
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![def_tool("web.search", &["search"], false)]);
    let obs = NoopMetrics;
    let input = base_input(anchor);

    let result = engine.run(input, &tw, &obs);
    assert!(matches!(result.unwrap_err(), EngineError::Tool(_)));
}

#[test]
fn self_reflection_lane_blocks_medium_risk_tools() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.scenario = Some(ConversationScenario::AiSelfTalk);
    let engine = default_engine(ToolsConfig::default());
    let tw = TwClientMock::with_defs(vec![
        def_tool("introspect.deep", &["self_reflection"], false),
        def_tool("analysis.generic", &["analysis"], false),
    ]);
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.anchor = anchor;
    input.scene = "self_reflection".into();
    input.capability_hints = vec!["self_reflection".into()];
    input.context_tags = json!({
        "stats": {
            "introspect.deep": {
                "success_rate": 0.9,
                "latency_p95": 0.2,
                "cost_per_success": 0.3,
                "risk_level": "very_low"
            },
            "analysis.generic": {
                "success_rate": 0.8,
                "latency_p95": 0.5,
                "cost_per_success": 0.6,
                "risk_level": "medium"
            }
        }
    });
    input.request_context = json!({
        "inputs": {
            "introspect.deep": { "prompt": "reflect" },
            "analysis.generic": { "prompt": "reflect" }
        }
    });

    let output = engine
        .run(input, &tw, &obs)
        .expect("plan for self reflection");
    assert_eq!(output.plan.lane, ToolLane::SelfReflection);
    assert_eq!(output.plan.strategy.mode, OrchestrationMode::Serial);
    assert_eq!(output.plan.items.len(), 1);
    assert_eq!(output.plan.items[0].tool_id, "introspect.deep");
    assert_eq!(output.dialogue_events.len(), 3);
    assert_eq!(
        output
            .dialogue_events
            .last()
            .map(|ev| ev.event_type.clone()),
        Some(DialogueEventType::SelfReflection)
    );
}

#[test]
fn collaboration_lane_allows_medium_risk_with_low_config_gate() {
    let mut anchor = TEST_ANCHOR.clone();
    anchor.scenario = Some(ConversationScenario::AiToAi);
    let mut config = ToolsConfig::default();
    config.router.risk_threshold = "low".into();
    let base_cost = config.budget.max_cost_tokens;
    let engine = default_engine(config.clone());
    let tw = TwClientMock::with_defs(vec![def_tool("collab.alpha", &["collaboration"], false)]);
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.anchor = anchor;
    input.scene = "collaboration".into();
    input.capability_hints = vec!["collaboration".into()];
    input.context_tags = json!({
        "stats": {
            "collab.alpha": {
                "success_rate": 0.85,
                "latency_p95": 0.6,
                "cost_per_success": 0.7,
                "risk_level": "medium"
            }
        }
    });
    input.request_context = json!({
        "inputs": {
            "collab.alpha": { "task": "brainstorm" }
        }
    });

    let output = engine
        .run(input, &tw, &obs)
        .expect("plan for collaboration");
    assert_eq!(output.plan.lane, ToolLane::Collaboration);
    assert!(output.plan.budget.max_cost_tokens > base_cost);
    assert_eq!(output.plan.items.len(), 1);
    assert_eq!(output.plan.items[0].tool_id, "collab.alpha");
    assert_eq!(output.dialogue_events.len(), 3);
    assert_eq!(
        output
            .dialogue_events
            .last()
            .map(|ev| ev.event_type.clone()),
        Some(DialogueEventType::ToolResult)
    );
}

#[test]
fn serial_mode_stops_after_first_success() {
    let anchor = TEST_ANCHOR.clone();
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 2;
    config.orchestrator.mode = OrchestrationMode::Serial;
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![
        def_tool("web.search", &["search"], false),
        def_tool("web.cache", &["search"], false),
    ]);
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "web.search".into(),
            schema_v: 1,
            summary: json!({"hits": 3}),
            evidence_pointer: None,
            result_digest: "digest-search".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("search#1".into()),
    }));
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.request_context["inputs"]["web.cache"] = json!({"query": "soulseed"});

    let output = engine.run(input, &tw, &obs).unwrap();
    assert_eq!(output.plan.strategy.mode, OrchestrationMode::Serial);
    assert_eq!(output.plan.items.len(), 2);
    assert_eq!(output.execution.explain_run.collected_summaries.len(), 1);
    assert_eq!(output.dialogue_events.len(), 2);

    let events = tw.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, TwEvent::ToolResponded { tool_id, .. } if tool_id == "web.search")));
    assert!(events
        .iter()
        .all(|e| !matches!(e, TwEvent::ToolResponded { tool_id, .. } if tool_id == "web.cache")));
}

#[test]
fn parallel_mode_collects_multiple_results() {
    let anchor = TEST_ANCHOR.clone();
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 2;
    config.orchestrator.mode = OrchestrationMode::Parallel;
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![
        def_tool("web.search", &["search"], false),
        def_tool("web.cache", &["search"], false),
    ]);
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "web.search".into(),
            schema_v: 1,
            summary: json!({"hits": 3}),
            evidence_pointer: None,
            result_digest: "digest-search".into(),
        },
        degradation_reason: None,
        indices_used: Some(vec!["idx_search".into()]),
        query_hash: Some("search#1".into()),
    }));
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "web.cache".into(),
            schema_v: 1,
            summary: json!({"cached": true}),
            evidence_pointer: None,
            result_digest: "digest-cache".into(),
        },
        degradation_reason: None,
        indices_used: Some(vec!["idx_cache".into()]),
        query_hash: Some("cache#1".into()),
    }));
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.request_context["inputs"]["web.cache"] = json!({"query": "soulseed"});

    let output = engine.run(input, &tw, &obs).unwrap();
    assert_eq!(output.plan.strategy.mode, OrchestrationMode::Parallel);
    assert_eq!(output.execution.explain_run.collected_summaries.len(), 2);
    assert_eq!(
        output
            .execution
            .explain_run
            .collected_summaries
            .iter()
            .map(|s| s.tool_id.as_str())
            .collect::<Vec<_>>(),
        vec!["web.search", "web.cache"]
    );
    assert!(!output.execution.fallback_triggered);
    assert_eq!(output.dialogue_events.len(), 4);
}

#[test]
fn hybrid_mode_runs_stream_after_success() {
    let anchor = TEST_ANCHOR.clone();
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 2;
    config.orchestrator.mode = OrchestrationMode::Hybrid;
    let engine = default_engine(config);
    let mut stream_tool = def_tool("stream.supplement", &["search"], false);
    stream_tool.supports_stream = true;
    let mut primary_tool = def_tool("primary.core", &["search"], false);
    primary_tool.supports_stream = false;
    primary_tool.risk_level = "medium".into();
    let tw = TwClientMock::with_defs(vec![primary_tool, stream_tool]);
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "primary.core".into(),
            schema_v: 1,
            summary: json!({"primary": true}),
            evidence_pointer: None,
            result_digest: "digest-primary".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("primary#1".into()),
    }));
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "stream.supplement".into(),
            schema_v: 1,
            summary: json!({"extra": true}),
            evidence_pointer: None,
            result_digest: "digest-stream".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("stream#1".into()),
    }));
    let obs = NoopMetrics;

    let mut input = base_input(anchor.clone());
    input.request_context["inputs"]["stream.supplement"] = json!({"query": "soulseed"});

    let output = engine.run(input, &tw, &obs).unwrap();
    assert_eq!(output.plan.strategy.mode, OrchestrationMode::Hybrid);
    assert_eq!(output.execution.explain_run.collected_summaries.len(), 2);
    let post_stage = output
        .execution
        .explain_run
        .stages
        .iter()
        .find(|stage| stage.tool_id == "stream.supplement")
        .expect("stream stage present");
    assert_eq!(post_stage.notes.as_deref(), Some("post_success"));
    assert_eq!(output.dialogue_events.len(), 4);
}

#[test]
fn request_context_graph_overrides_default_plan() {
    let anchor = TEST_ANCHOR.clone();
    let mut config = ToolsConfig::default();
    config.router.max_candidates = 3;
    config.orchestrator.mode = OrchestrationMode::Parallel;
    let engine = default_engine(config);
    let tw = TwClientMock::with_defs(vec![
        def_tool("alpha.tool", &["search"], false),
        def_tool("beta.tool", &["search"], false),
        def_tool("gamma.tool", &["search"], false),
    ]);
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "alpha.tool".into(),
            schema_v: 1,
            summary: json!({"alpha": true}),
            evidence_pointer: None,
            result_digest: "digest-alpha".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("alpha#1".into()),
    }));
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "beta.tool".into(),
            schema_v: 1,
            summary: json!({"beta": true}),
            evidence_pointer: None,
            result_digest: "digest-beta".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("beta#1".into()),
    }));
    tw.push_execute(Ok(TwExecuteResult {
        summary: ToolResultSummary {
            tool_id: "gamma.tool".into(),
            schema_v: 1,
            summary: json!({"gamma": true}),
            evidence_pointer: None,
            result_digest: "digest-gamma".into(),
        },
        degradation_reason: None,
        indices_used: None,
        query_hash: Some("gamma#1".into()),
    }));
    let obs = NoopMetrics;

    let input = EngineInput {
        anchor,
        scene: "graph_test".into(),
        capability_hints: vec!["search".into()],
        context_tags: json!({
            "stats": {
                "alpha.tool": {"success_rate": 0.92, "latency_p95": 0.3, "cost_per_success": 0.5, "risk_level": "low"},
                "beta.tool": {"success_rate": 0.85, "latency_p95": 0.4, "cost_per_success": 0.6, "risk_level": "medium"},
                "gamma.tool": {"success_rate": 0.8, "latency_p95": 0.45, "cost_per_success": 0.65, "risk_level": "medium"}
            }
        }),
        request_context: json!({
            "inputs": {
                "alpha.tool": {"query": "a"},
                "beta.tool": {"query": "b"},
                "gamma.tool": {"query": "c"}
            },
            "graph": {
                "edges": [
                    {"from": 0, "to": 2, "kind": "sequential"},
                    {"from": 0, "to": 1, "kind": "dependency"}
                ],
                "barriers": [
                    {"nodes": [1, 2]}
                ]
            }
        }),
        subject: None,
    };

    let output = engine.run(input, &tw, &obs).expect("graph plan");
    assert_eq!(output.plan.items.len(), 3);
    assert!(output
        .plan
        .graph
        .edges
        .iter()
        .any(|edge| edge.from == 0 && edge.to == 2 && edge.kind == ToolEdgeKind::Sequential));
    assert!(output
        .plan
        .graph
        .edges
        .iter()
        .any(|edge| edge.from == 0 && edge.to == 1 && edge.kind == ToolEdgeKind::Dependency));
    assert_eq!(output.plan.graph.barriers.len(), 1);
    assert_eq!(output.plan.graph.barriers[0].nodes, vec![1, 2]);
    assert_eq!(
        output.plan.graph.barriers[0].barrier_id,
        format!("{}::barrier-0", output.plan.plan_id)
    );
    assert_eq!(output.execution.explain_run.collected_summaries.len(), 3);
}

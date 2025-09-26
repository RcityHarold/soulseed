use serde_json::json;
use soulseed_agi_context::types::{
    Anchor as ContextAnchor, BudgetSummary, BundleSegment, ContextBundle, ExplainBundle, Partition,
};
use soulseed_agi_core_models::{
    AccessClass, ConversationScenario, Provenance, SessionId, TenantId,
    awareness::{
        AwarenessAnchor, AwarenessDegradationReason, AwarenessFork, ClarifyLimits, ClarifyPlan,
        CollabPlan, DecisionPlan, SelfPlan, ToolPlan, ToolPlanBarrier, ToolPlanEdge, ToolPlanNode,
    },
};
use soulseed_agi_dfr::types::RoutePlan;
use soulseed_agi_dfr::{AuthzService, QuotaService};
use soulseed_agi_dfr::{
    CandidateFilter, DfrEngine, RoutePlanner, RouterCandidate, RouterInput, RouterService,
};
use uuid::Uuid;

fn awareness_anchor(access: AccessClass, provenance: Option<Provenance>) -> AwarenessAnchor {
    AwarenessAnchor {
        tenant_id: TenantId::new(42),
        envelope_id: Uuid::nil(),
        config_snapshot_hash: "cfg:v1".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::new(7)),
        sequence_number: Some(1),
        access_class: access,
        provenance,
        schema_v: 1,
    }
}

fn context_anchor(
    access: AccessClass,
    provenance: Option<Provenance>,
    scenario: ConversationScenario,
) -> ContextAnchor {
    ContextAnchor {
        tenant_id: TenantId::new(42),
        envelope_id: Uuid::nil(),
        config_snapshot_hash: "cfg:v1".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::new(7)),
        sequence_number: Some(1),
        access_class: access,
        provenance,
        schema_v: 1,
        scenario: Some(scenario),
    }
}

fn context_bundle(
    access: AccessClass,
    provenance: Option<Provenance>,
    scenario: ConversationScenario,
    degradation: Option<&str>,
    indices: &[&str],
    query_hash: Option<&str>,
) -> (ContextBundle, String) {
    let explain = ExplainBundle {
        reasons: vec!["context_filter".into()],
        degradation_reason: degradation.map(|s| s.to_string()),
        indices_used: indices.iter().map(|s| s.to_string()).collect(),
        query_hash: query_hash.map(|s| s.to_string()),
    };

    let bundle = ContextBundle {
        anchor: context_anchor(access, provenance, scenario.clone()),
        segments: vec![BundleSegment {
            partition: Partition::P4Dialogue,
            items: Vec::new(),
        }],
        explain,
        budget: BudgetSummary {
            target_tokens: 800,
            projected_tokens: 480,
        },
    };

    (bundle, format!("sha256:{}", Uuid::nil()))
}

fn router_input(
    access: AccessClass,
    provenance: Option<Provenance>,
    degradation: Option<&str>,
    indices: &[&str],
    query_hash: Option<&str>,
) -> RouterInput {
    let scenario = ConversationScenario::HumanToAi;
    let (bundle, digest) = context_bundle(
        access,
        provenance.clone(),
        scenario.clone(),
        degradation,
        indices,
        query_hash,
    );
    RouterInput {
        anchor: awareness_anchor(access, provenance),
        context_digest: digest,
        context: bundle,
        scenario,
        scene_label: "demo.scene".into(),
        user_prompt: "hello".into(),
        tags: json!({}),
    }
}

struct AllowAuthz;

impl AuthzService for AllowAuthz {
    fn authorize(
        &self,
        _input: &RouterInput,
        _plan: &RoutePlan,
    ) -> Result<(), soulseed_agi_dfr::DfrError> {
        Ok(())
    }
}

struct AllowQuota;

impl QuotaService for AllowQuota {
    fn consume(
        &self,
        _input: &RouterInput,
        _plan: &RoutePlan,
    ) -> Result<(), soulseed_agi_dfr::DfrError> {
        Ok(())
    }
}

#[test]
fn filter_rejects_restricted_without_provenance() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let input = router_input(
        AccessClass::Restricted,
        None,
        None,
        &["timeline"],
        Some("qhash"),
    );

    let candidate = RouterCandidate {
        decision_plan: DecisionPlan::SelfReason {
            plan: SelfPlan {
                hint: Some("focus".into()),
                max_ic: Some(2),
            },
        },
        fork: AwarenessFork::SelfReason,
        priority: 0.9,
        metadata: json!({ "label": "self" }),
    };

    let (outcome, plans) = router.evaluate(&input, vec![candidate]).expect("evaluate");
    assert!(plans.is_empty());
    assert!(outcome.accepted.is_empty());
    assert_eq!(outcome.rejected.len(), 1);
    assert_eq!(outcome.rejected[0].0, "privacy_restricted");
}

#[test]
fn engine_emits_budget_and_explain_details() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "user".into(),
        method: "oauth".into(),
        model: None,
        content_digest_sha256: None,
    });
    let input = router_input(
        AccessClass::Internal,
        provenance,
        Some("graph:lag"),
        &["timeline"],
        Some("graph:q1"),
    );
    let digest = input.context_digest.clone();

    let candidate = RouterCandidate {
        decision_plan: DecisionPlan::SelfReason {
            plan: SelfPlan {
                hint: Some("deep".into()),
                max_ic: Some(3),
            },
        },
        fork: AwarenessFork::SelfReason,
        priority: 0.72,
        metadata: json!({
            "label": "self_route",
            "indices_used": ["timeline", "causal"],
            "query_hash": "blake3:abcdef",
            "degrade_hint": "budget_tokens",
            "estimate": {
                "tokens": 600,
                "walltime_ms": 1200,
                "external_cost": 0.75,
                "risk": 0.2
            }
        }),
    };

    let decision = engine
        .route(input, vec![candidate])
        .expect("route successful");

    assert_eq!(decision.context_digest, digest);
    assert!(decision.plan.explain.router_digest.starts_with("blake3:"));
    assert!(
        decision
            .plan
            .explain
            .router_config_digest
            .starts_with("blake3:")
    );
    assert_eq!(
        decision.plan.explain.indices_used,
        vec!["timeline", "causal"]
    );
    assert_eq!(
        decision.plan.explain.query_hash.as_deref(),
        Some("blake3:abcdef")
    );
    assert_eq!(
        decision.plan.explain.degradation_reason.as_deref(),
        Some("budget_tokens")
    );
    assert_eq!(
        decision.decision_path.degradation_reason,
        Some(AwarenessDegradationReason::BudgetTokens)
    );
    assert_eq!(decision.decision_path.budget_plan.tokens, Some(600));
    assert_eq!(decision.decision_path.budget_plan.walltime_ms, Some(1200));
    assert_eq!(decision.decision_path.budget_plan.external_cost, Some(0.75));
    let diag = decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics object");
    assert_eq!(
        diag.get("route_oscillation").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert!(decision.rejected.is_empty());
}

#[test]
fn clarify_timeout_maps_to_clarify_exhausted() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "ops".into(),
        method: "console".into(),
        model: None,
        content_digest_sha256: None,
    });
    let input = router_input(
        AccessClass::Internal,
        provenance,
        Some("clarify:pending"),
        &["timeline"],
        Some("clarify:q"),
    );

    let candidate = RouterCandidate {
        decision_plan: DecisionPlan::Clarify {
            plan: ClarifyPlan {
                questions: vec![],
                limits: ClarifyLimits {
                    max_parallel: Some(1),
                    max_rounds: Some(1),
                    wait_ms: Some(2000),
                    total_wait_ms: Some(3000),
                },
            },
        },
        fork: AwarenessFork::Clarify,
        priority: 0.65,
        metadata: json!({
            "label": "clarify_timeout",
            "degrade_hint": "clarify_timeout|timeout_fallback",
            "diagnostics": {"clarify": "timeout"},
        }),
    };

    let decision = engine
        .route(input, vec![candidate])
        .expect("route clarify timeout");

    assert_eq!(decision.plan.fork, AwarenessFork::Clarify);
    assert_eq!(
        decision.plan.explain.degradation_reason.as_deref(),
        Some("clarify_timeout|timeout_fallback")
    );
    assert_eq!(
        decision.decision_path.degradation_reason,
        Some(AwarenessDegradationReason::ClarifyExhausted)
    );
    assert!(decision.rejected.is_empty());
}

#[test]
fn tool_plan_barrier_preserved_and_timeout_mapped() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "bot".into(),
        method: "auto".into(),
        model: Some("planner".into()),
        content_digest_sha256: None,
    });

    let input = router_input(
        AccessClass::Internal,
        provenance,
        None,
        &["timeline", "tools"],
        Some("tool:qhash"),
    );

    let candidate = RouterCandidate {
        decision_plan: DecisionPlan::Tool {
            plan: ToolPlan {
                nodes: vec![
                    ToolPlanNode {
                        id: "search".into(),
                        tool_id: "web.search".into(),
                        version: Some("1".into()),
                        input: json!({"query": "Soulseed"}),
                        timeout_ms: Some(900),
                        success_criteria: None,
                        evidence_policy: Some("standard".into()),
                    },
                    ToolPlanNode {
                        id: "summarize".into(),
                        tool_id: "summarizer".into(),
                        version: Some("1".into()),
                        input: json!({"mode": "brief"}),
                        timeout_ms: Some(600),
                        success_criteria: None,
                        evidence_policy: None,
                    },
                ],
                edges: vec![ToolPlanEdge {
                    from: "search".into(),
                    to: "summarize".into(),
                }],
                barrier: ToolPlanBarrier {
                    mode: Some("parallel".into()),
                    timeout_ms: Some(450),
                },
            },
        },
        fork: AwarenessFork::ToolPath,
        priority: 0.78,
        metadata: json!({
            "label": "tool_parallel",
            "degrade_hint": "tool_timeout",
            "indices_used": ["timeline", "tools"],
            "diagnostics": {"barrier": "parallel"},
        }),
    };

    let decision = engine.route(input, vec![candidate]).expect("route tool");

    assert_eq!(decision.plan.fork, AwarenessFork::ToolPath);
    assert_eq!(
        decision.plan.explain.degradation_reason.as_deref(),
        Some("tool_timeout")
    );
    assert_eq!(
        decision.decision_path.degradation_reason,
        Some(AwarenessDegradationReason::BudgetWalltime)
    );

    if let DecisionPlan::Tool { plan } = &decision.decision_path.plan {
        assert_eq!(plan.barrier.mode.as_deref(), Some("parallel"));
        assert_eq!(plan.barrier.timeout_ms, Some(450));
        assert_eq!(plan.nodes.len(), 2);
        assert!(
            plan.edges
                .iter()
                .any(|edge| edge.from == "search" && edge.to == "summarize")
        );
    } else {
        panic!("expected tool plan");
    }
}

#[test]
fn explain_falls_back_to_context_when_candidate_missing_fields() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "human".into(),
        method: "manual".into(),
        model: None,
        content_digest_sha256: None,
    });
    let input = router_input(
        AccessClass::Internal,
        provenance,
        Some("envctx:overload"),
        &["timeline", "semantic"],
        Some("graph:q2"),
    );
    let digest = input.context_digest.clone();

    let candidate = RouterCandidate {
        decision_plan: DecisionPlan::Clarify {
            plan: ClarifyPlan {
                questions: Vec::new(),
                limits: ClarifyLimits::default(),
            },
        },
        fork: AwarenessFork::Clarify,
        priority: 0.61,
        metadata: json!({
            "label": "clarify_default",
            "estimate": {
                "tokens": 320,
                "walltime_ms": 900,
                "external_cost": 0.1,
                "risk": 0.1
            }
        }),
    };

    let decision = engine
        .route(input, vec![candidate])
        .expect("route successful");

    assert_eq!(decision.context_digest, digest);
    assert_eq!(
        decision.plan.explain.indices_used,
        vec!["timeline", "semantic"]
    );
    assert_eq!(
        decision.plan.explain.query_hash.as_deref(),
        Some("graph:q2")
    );
    assert_eq!(
        decision.plan.explain.degradation_reason.as_deref(),
        Some("envctx:overload")
    );
    assert_eq!(
        decision.decision_path.degradation_reason,
        Some(AwarenessDegradationReason::EnvctxDegraded)
    );
    assert!(
        decision
            .decision_path
            .rationale
            .thresholds_hit
            .iter()
            .any(|tag| tag == "candidate_degrade:envctx:overload")
    );
    assert_eq!(decision.decision_path.budget_plan.tokens, Some(320));
    let diag = decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics object");
    assert_eq!(
        diag.get("route_oscillation").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert!(decision.rejected.is_empty());
}

#[test]
fn stickiness_and_oscillation_tracking() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "ops".into(),
        method: "console".into(),
        model: None,
        content_digest_sha256: None,
    });

    let clarify_candidate = |priority: f32| RouterCandidate {
        decision_plan: DecisionPlan::Clarify {
            plan: ClarifyPlan {
                questions: Vec::new(),
                limits: ClarifyLimits::default(),
            },
        },
        fork: AwarenessFork::Clarify,
        priority,
        metadata: json!({ "label": "clarify" }),
    };

    let tool_candidate = |priority: f32| RouterCandidate {
        decision_plan: DecisionPlan::Tool {
            plan: ToolPlan {
                nodes: vec![ToolPlanNode {
                    id: "tool".into(),
                    tool_id: "search".into(),
                    version: None,
                    input: json!({}),
                    timeout_ms: None,
                    success_criteria: None,
                    evidence_policy: None,
                }],
                edges: Vec::new(),
                barrier: ToolPlanBarrier::default(),
            },
        },
        fork: AwarenessFork::ToolPath,
        priority,
        metadata: json!({ "label": "tool" }),
    };

    let first = router_input(
        AccessClass::Internal,
        provenance.clone(),
        None,
        &["timeline"],
        Some("graph:stick"),
    );
    let first_decision = engine
        .route(first, vec![clarify_candidate(0.8), tool_candidate(0.6)])
        .expect("first route");
    assert_eq!(first_decision.plan.fork, AwarenessFork::Clarify);
    let diag_first = first_decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics");
    assert_eq!(
        diag_first.get("route_oscillation").and_then(|v| v.as_u64()),
        Some(0)
    );

    let second = router_input(
        AccessClass::Internal,
        provenance.clone(),
        None,
        &["timeline"],
        Some("graph:stick"),
    );
    let second_decision = engine
        .route(second, vec![clarify_candidate(0.55), tool_candidate(0.6)])
        .expect("second route");
    assert_eq!(second_decision.plan.fork, AwarenessFork::Clarify);
    let diag_second = second_decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics");
    assert_eq!(
        diag_second
            .get("route_oscillation")
            .and_then(|v| v.as_u64()),
        Some(0)
    );

    let third = router_input(
        AccessClass::Internal,
        provenance,
        None,
        &["timeline"],
        Some("graph:stick"),
    );
    let third_decision = engine
        .route(third, vec![clarify_candidate(0.2), tool_candidate(0.9)])
        .expect("third route");
    assert_eq!(third_decision.plan.fork, AwarenessFork::ToolPath);
    let diag_third = third_decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics");
    assert_eq!(
        diag_third.get("route_oscillation").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(
        third_decision
            .decision_path
            .rationale
            .thresholds_hit
            .iter()
            .any(|tag| tag == "route_oscillation:1")
    );
}

#[test]
fn plan_validation_fallbacks_to_clarify() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "ops".into(),
        method: "console".into(),
        model: None,
        content_digest_sha256: None,
    });

    let input = router_input(
        AccessClass::Internal,
        provenance,
        None,
        &["timeline"],
        Some("graph:fallback"),
    );

    let invalid_tool = RouterCandidate {
        decision_plan: DecisionPlan::Tool {
            plan: ToolPlan {
                nodes: vec![ToolPlanNode {
                    id: "tool".into(),
                    tool_id: "calc".into(),
                    version: Some("1".into()),
                    input: json!({ "a": 1, "b": 2 }),
                    timeout_ms: Some(500),
                    success_criteria: None,
                    evidence_policy: None,
                }],
                edges: Vec::new(),
                barrier: ToolPlanBarrier::default(),
            },
        },
        fork: AwarenessFork::ToolPath,
        priority: 0.9,
        metadata: json!({
            "label": "tool_invalid",
            "validation": {
                "status": "invalid",
                "reason": "tool_schema_error"
            },
            "indices_used": ["timeline"],
            "query_hash": "blake3:toolbad",
        }),
    };

    let decision = engine
        .route(input, vec![invalid_tool])
        .expect("route fallback");

    assert_eq!(decision.plan.fork, AwarenessFork::Clarify);
    assert_eq!(
        decision.plan.explain.degradation_reason.as_deref(),
        Some("invalid_plan")
    );
    assert_eq!(
        decision.decision_path.degradation_reason,
        Some(AwarenessDegradationReason::InvalidPlan)
    );
    let diag = decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics");
    assert_eq!(diag.get("fallback"), Some(&json!(true)));
    assert_eq!(
        diag.get("invalid_candidates")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str()),
        Some("tool_schema_error")
    );
    assert!(
        decision
            .decision_path
            .rationale
            .thresholds_hit
            .iter()
            .any(|tag| tag == "candidate_degrade:invalid_plan")
    );
    assert!(
        decision
            .rejected
            .iter()
            .any(|(code, reason)| code == "plan_invalid" && reason == "tool_schema_error")
    );
}

#[test]
fn collab_plan_routing() {
    let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
    let planner = RoutePlanner::default();
    let authz = AllowAuthz;
    let quota = AllowQuota;
    let engine = DfrEngine::new(&router, &planner, &authz, &quota);

    let provenance = Some(Provenance {
        source: "ops".into(),
        method: "console".into(),
        model: None,
        content_digest_sha256: None,
    });

    let input = router_input(
        AccessClass::Internal,
        provenance,
        None,
        &["timeline", "collab"],
        Some("graph:collab"),
    );

    let collab_candidate = RouterCandidate {
        decision_plan: DecisionPlan::Collab {
            plan: CollabPlan {
                scope: json!({ "channel": "team-alpha" }),
                order: Some("round_robin".into()),
                rounds: Some(2),
                privacy_mode: Some("shared".into()),
                barrier: ToolPlanBarrier::default(),
            },
        },
        fork: AwarenessFork::Collab,
        priority: 0.82,
        metadata: json!({
            "label": "collab_team",
            "indices_used": ["timeline", "collab"],
            "query_hash": "blake3:collab",
        }),
    };

    let tool_candidate = RouterCandidate {
        decision_plan: DecisionPlan::Tool {
            plan: ToolPlan {
                nodes: vec![ToolPlanNode {
                    id: "tool".into(),
                    tool_id: "search".into(),
                    version: Some("1".into()),
                    input: json!({}),
                    timeout_ms: Some(200),
                    success_criteria: None,
                    evidence_policy: None,
                }],
                edges: Vec::new(),
                barrier: ToolPlanBarrier::default(),
            },
        },
        fork: AwarenessFork::ToolPath,
        priority: 0.6,
        metadata: json!({ "label": "tool" }),
    };

    let decision = engine
        .route(input, vec![tool_candidate, collab_candidate])
        .expect("route collab");

    assert_eq!(decision.plan.fork, AwarenessFork::Collab);
    assert!(matches!(
        decision.decision_path.plan,
        DecisionPlan::Collab { .. }
    ));
    assert_eq!(decision.decision_path.degradation_reason, None);
    let diag = decision
        .plan
        .explain
        .diagnostics
        .as_object()
        .expect("diagnostics");
    assert_eq!(
        diag.get("route_oscillation").and_then(|v| v.as_u64()),
        Some(0)
    );
}

#[test]
fn explain_digest_is_stable() {
    fn run_once() -> soulseed_agi_dfr::RouterDecision {
        let router = RouterService::new(CandidateFilter::default(), RoutePlanner::default());
        let planner = RoutePlanner::default();
        let authz = AllowAuthz;
        let quota = AllowQuota;
        let engine = DfrEngine::new(&router, &planner, &authz, &quota);

        let provenance = Some(Provenance {
            source: "user".into(),
            method: "oauth".into(),
            model: None,
            content_digest_sha256: None,
        });

        let input = router_input(
            AccessClass::Internal,
            provenance,
            None,
            &["timeline"],
            Some("graph:stable"),
        );

        let candidate = RouterCandidate {
            decision_plan: DecisionPlan::SelfReason {
                plan: SelfPlan {
                    hint: Some("reflect".into()),
                    max_ic: Some(2),
                },
            },
            fork: AwarenessFork::SelfReason,
            priority: 0.7,
            metadata: json!({
                "label": "self_reflect",
                "indices_used": ["timeline"],
                "query_hash": "blake3:self",
            }),
        };

        engine.route(input, vec![candidate]).expect("route self")
    }

    let first = run_once();
    let second = run_once();

    assert_eq!(
        first.plan.explain.router_digest,
        second.plan.explain.router_digest
    );
    assert_eq!(
        first.plan.explain.router_config_digest,
        second.plan.explain.router_config_digest
    );
    assert_eq!(
        first.plan.explain.routing_seed,
        second.plan.explain.routing_seed
    );
    assert_eq!(
        first.decision_path.explain.router_digest,
        second.decision_path.explain.router_digest
    );
}

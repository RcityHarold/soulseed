use serde_json::json;
#[cfg(feature = "vectors-extra")]
use soulseed_agi_core_models::ExtraVectors;
use soulseed_agi_core_models::awareness::{
    AwarenessAnchor, AwarenessDegradationReason, AwarenessFork, ClarifyLimits, ClarifyPlan,
    CollabPlan, DecisionBudgetEstimate, DecisionExplain, DecisionPath as AwarenessDecisionPath,
    DecisionPlan as AwarenessDecisionPlan, DecisionRationale, SelfPlan, ToolPlan, ToolPlanBarrier,
};
use soulseed_agi_core_models::{
    AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
    EnvelopeHead, EventId, Provenance, RealTimePriority, Snapshot, Subject, SubjectRef, TraceId,
};
use soulseed_agi_core_models::common::EvidencePointer;
use soulseed_agi_dfr::types::{BudgetEstimate, RouteExplain, RoutePlan, RouterDecision};
use soulseed_agi_llm::{
    dto::{
        LlmInput, LlmResult, ModelProfile, PromptSegment, ReasoningVisibility, TokenUsage,
    },
    engine::{LlmConfig, LlmEngine},
    integrator::{LlmIntegrationOptions, build_llm_input},
    tw_client::MockThinWaistClient,
};
use soulseed_agi_tools::dto::{
    Anchor, EvidencePointer as ToolEvidencePointer, SessionId, SummaryLineage, TenantId,
    ToolResultSummary,
};
use time::OffsetDateTime;
use uuid::Uuid;

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId(9),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg-tools-llm".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(77)),
        sequence_number: Some(5),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "router".into(),
            method: "clarify_flow".into(),
            model: Some("planner_v1".into()),
            content_digest_sha256: Some("sha256:context".into()),
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    }
}

fn clarify_event(anchor: &Anchor, text: &str) -> DialogueEvent {
    let now = OffsetDateTime::now_utc();
    let scenario = anchor
        .scenario
        .clone()
        .unwrap_or(ConversationScenario::HumanToAi);
    DialogueEvent {
        tenant_id: anchor.tenant_id,
        event_id: EventId(now.unix_timestamp_nanos() as u64),
        session_id: anchor.session_id.unwrap(),
        subject: Subject::Human(soulseed_agi_core_models::HumanId(777)),
        participants: vec![SubjectRef {
            kind: Subject::AI(soulseed_agi_core_models::AIId::new(0)),
            role: Some("assistant".into()),
        }],
        head: EnvelopeHead {
            envelope_id: anchor.envelope_id,
            trace_id: TraceId(format!("clarify:{}", anchor.envelope_id)),
            correlation_id: CorrelationId("clarify-run".into()),
            config_snapshot_hash: anchor.config_snapshot_hash.clone(),
            config_snapshot_version: anchor.config_snapshot_version,
        },
        snapshot: Snapshot {
            schema_v: anchor.schema_v,
            created_at: now,
        },
        timestamp_ms: (now.unix_timestamp_nanos() / 1_000_000) as i64,
        scenario,
        event_type: DialogueEventType::Message,
        time_window: None,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        sequence_number: anchor.sequence_number.unwrap_or(1),
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: vec![],
        reasoning_trace: None,
        reasoning_confidence: None,
        reasoning_strategy: None,
        content_embedding: None,
        context_embedding: None,
        decision_embedding: None,
        embedding_meta: None,
        concept_vector: None,
        semantic_cluster_id: None,
        cluster_method: None,
        concept_distance_to_goal: None,
        real_time_priority: Some(RealTimePriority::Normal),
        notification_targets: None,
        live_stream_id: None,
        growth_stage: None,
        processing_latency_ms: None,
        influence_score: None,
        community_impact: None,
        evidence_pointer: None,
        content_digest_sha256: None,
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(soulseed_agi_core_models::MessagePointer {
            message_id: soulseed_agi_core_models::MessageId(now.unix_timestamp() as u64),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: json!({"clarify": true, "text": text}),
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    }
}

fn tool_event(
    anchor: &Anchor,
    event_type: DialogueEventType,
    metadata: serde_json::Value,
) -> DialogueEvent {
    let mut event = clarify_event(anchor, "");
    event.event_type = event_type;
    event.subject = Subject::AI(soulseed_agi_core_models::AIId::new(0));
    event.participants = vec![SubjectRef {
        kind: Subject::Human(soulseed_agi_core_models::HumanId(777)),
        role: Some("requester".into()),
    }];
    event.metadata = metadata;
    event
}

#[test]
fn clarify_tool_llm_pipeline() {
    let anchor = anchor();
    let clarify = "请确认你希望我总结工具执行结果。";

    let tool_degrade = Some("timeout_fallback".to_string());
    let tool_indices = Some(vec![String::from("timeline_v1")]);
    let tool_query_hash = Some("timeline#clarify".to_string());
    let tool_meta = json!({
        "degradation_reason": tool_degrade.clone().unwrap(),
        "indices_used": tool_indices.clone().unwrap(),
        "query_hash": tool_query_hash.clone().unwrap(),
    });
    let tool_dialogue_events = vec![
        tool_event(&anchor, DialogueEventType::ToolCall, tool_meta.clone()),
        tool_event(&anchor, DialogueEventType::ToolResult, tool_meta.clone()),
    ];

    let call_meta = tool_dialogue_events[0].metadata.as_object().expect("call meta");
    assert_eq!(
        call_meta
            .get("degradation_reason")
            .and_then(|v| v.as_str())
            .unwrap(),
        "timeout_fallback"
    );
    assert_eq!(
        call_meta
            .get("indices_used")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["timeline_v1"]
    );
    assert_eq!(
        call_meta
            .get("query_hash")
            .and_then(|v| v.as_str())
            .unwrap(),
        "timeline#clarify"
    );

    let result_meta = tool_dialogue_events[1]
        .metadata
        .as_object()
        .expect("result meta");
    assert_eq!(
        result_meta
            .get("degradation_reason")
            .and_then(|v| v.as_str())
            .unwrap(),
        "timeout_fallback"
    );
    assert_eq!(
        result_meta
            .get("indices_used")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["timeline_v1"]
    );
    assert_eq!(
        result_meta
            .get("query_hash")
            .and_then(|v| v.as_str())
            .unwrap(),
        "timeline#clarify"
    );

    let tool_summary = Some(ToolResultSummary {
        tool_id: "web.search".into(),
        schema_v: 1,
        summary: json!({"hits": 3, "top": "Soulseed"}),
        evidence_pointer: ToolEvidencePointer {
            uri: "s3://search/result.json".into(),
            digest_sha256: Some("sha256:tool".into()),
            media_type: Some("application/json".into()),
            blob_ref: None,
            span: None,
            access_policy: Some("internal".into()),
        },
        result_digest: "digest-search".into(),
        lineage: SummaryLineage::default(),
    });

    let model = ModelProfile {
        model_id: "llm.soulseed.v1".into(),
        policy_digest: "sha256:llm-policy".into(),
        safety_tier: "standard".into(),
        max_output_tokens: 2048,
        selection_rank: Some(1),
        selection_score: Some(0.82),
        usage_band: Some("standard".into()),
        estimated_cost_usd: Some(0.0032),
        should_use_factors: json!({"quality": 0.9, "latency_ms": 820}),
    };
    let llm_client = MockThinWaistClient::new(model.clone());
    llm_client.push_result(Ok(LlmResult {
        completion: "工具已成功执行，结果简介如下：...".into(),
        summary: Some("工具执行成功，返回经压缩的最终答复".into()),
        evidence_pointer: Some(EvidencePointer {
            uri: "soulbase://llm/result/123".into(),
            digest_sha256: Some("sha256:llm-result-123".into()),
            media_type: Some("text/plain".into()),
            blob_ref: None,
            span: None,
            access_policy: Some("summary_only".into()),
        }),
        reasoning: vec![PromptSegment {
            role: "assistant".into(),
            content: "综合工具结果生成最终答复".into(),
        }],
        reasoning_visibility: ReasoningVisibility::TicketRequired,
        degradation_reason: Some("llm_timeout_recovered".into()),
        indices_used: Some(vec!["llm_cache_v1".into()]),
        query_hash: Some("llm#clarify".into()),
        usage: TokenUsage {
            prompt_tokens: 128,
            completion_tokens: 64,
            ..Default::default()
        },
    }));

    let llm_engine = LlmEngine::new(&llm_client, LlmConfig::default());
    let llm_input = LlmInput {
        anchor: anchor.clone(),
        scene: "clarify_tool".into(),
        clarify_prompt: Some(clarify.into()),
        tool_summary: tool_summary.clone(),
        user_prompt: "请生成最终结论".into(),
        context_tags: json!({"clarify": true}),
        degrade_hint: tool_degrade.clone(),
        tool_indices: tool_indices.clone(),
        tool_query_hash: tool_query_hash.clone(),
    };

    let llm_output = llm_engine.run(llm_input).expect("llm run");
    let final_event = llm_output.final_event.clone();

    let mut pipeline = vec![clarify_event(&anchor, clarify)];
    pipeline.extend(tool_dialogue_events.clone());
    pipeline.push(final_event.clone());

    assert_eq!(pipeline.len(), 4);
    assert_eq!(final_event.event_type, DialogueEventType::Message);
    assert!(
        final_event
            .metadata
            .get("degradation_chain")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("timeout_fallback")
    );
    assert!(
        final_event
            .metadata
            .get("model_id")
            .and_then(|v| v.as_str())
            .is_some()
    );
    assert_eq!(
        final_event
            .metadata
            .get("degradation_reason")
            .and_then(|v| v.as_str())
            .unwrap(),
        "llm_timeout_recovered"
    );
    assert_eq!(
        final_event
            .metadata
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap(),
        "工具执行成功，返回经压缩的最终答复"
    );
    assert_eq!(
        final_event
            .metadata
            .get("model_rank")
            .and_then(|v| v.as_u64())
            .unwrap(),
        1
    );
    assert!(
        final_event
            .metadata
            .get("should_use_factors")
            .unwrap()
            .get("quality")
            .is_some()
    );
    assert_eq!(
        final_event.evidence_pointer.as_ref().unwrap().uri,
        "soulbase://llm/result/123"
    );
    assert_eq!(
        llm_output.explain.usage_rank,
        Some(1),
        "should propagate rank into explain"
    );
    assert_eq!(llm_output.explain.usage_score, Some(0.82));
    assert_eq!(llm_output.explain.usage_band.as_deref(), Some("standard"));
    assert!(
        llm_output
            .explain
            .should_use_factors
            .get("quality")
            .is_some()
    );
    assert_eq!(
        llm_output
            .result
            .summary
            .as_deref()
            .expect("summary returned"),
        "工具执行成功，返回经压缩的最终答复"
    );

    let explain_reason = llm_output.explain.degradation_reason.as_deref().unwrap();
    assert!(explain_reason.contains("timeout_fallback"));
    assert!(explain_reason.contains("llm_timeout_recovered"));
    assert_eq!(
        llm_output
            .explain
            .indices_used
            .as_ref()
            .expect("indices used"),
        &vec![String::from("timeline_v1"), String::from("llm_cache_v1"),]
    );
    assert_eq!(
        llm_output
            .explain
            .query_hash
            .as_deref()
            .expect("query hash"),
        "timeline#clarify|llm#clarify"
    );

    let final_meta = final_event.metadata.as_object().expect("final metadata");
    assert_eq!(
        final_meta
            .get("indices_used")
            .and_then(|v| v.as_array())
            .expect("indices in metadata")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec![String::from("timeline_v1"), String::from("llm_cache_v1"),]
    );
    assert_eq!(
        final_meta
            .get("query_hash")
            .and_then(|v| v.as_str())
            .expect("metadata query hash"),
        "timeline#clarify|llm#clarify"
    );

    let reconciles = llm_client.take_reconciles();
    assert!(!reconciles.is_empty());
}

#[test]
fn build_llm_input_from_dfr_decision() {
    let awareness_anchor = AwarenessAnchor {
        tenant_id: TenantId::new(99),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg:dfr".into(),
        config_snapshot_version: 2,
        session_id: Some(SessionId::new(123)),
        sequence_number: Some(11),
        access_class: AccessClass::Internal,
        provenance: Some(Provenance {
            source: "router".into(),
            method: "dfr".into(),
            model: Some("router_v1".into()),
            content_digest_sha256: None,
        }),
        schema_v: 1,
    };

    let route_plan = RoutePlan {
        cycle_id: soulseed_agi_core_models::CycleId(1),
        anchor: awareness_anchor.clone(),
        fork: AwarenessFork::ToolPath,
        decision_plan: AwarenessDecisionPlan::Tool {
            plan: ToolPlan {
                nodes: Vec::new(),
                edges: Vec::new(),
                barrier: ToolPlanBarrier::default(),
            },
        },
        budget: BudgetEstimate {
            tokens: 512,
            walltime_ms: 800,
            external_cost: 1.5,
        },
        priority: 0.72,
        explain: RouteExplain {
            routing_seed: 42,
            router_digest: "blake3:router".into(),
            router_config_digest: "blake3:config".into(),
            indices_used: vec!["timeline_v2".into()],
            query_hash: Some("dfr#route".into()),
            degradation_reason: Some("graph_degraded".into()),
            diagnostics: json!({"route": "tool"}),
            rejected: vec![],
        },
    };

    let decision_path = AwarenessDecisionPath {
        anchor: awareness_anchor.clone(),
        awareness_cycle_id: soulseed_agi_core_models::CycleId(1),
        inference_cycle_sequence: 1,
        fork: AwarenessFork::ToolPath,
        plan: AwarenessDecisionPlan::Tool {
            plan: ToolPlan {
                nodes: Vec::new(),
                edges: Vec::new(),
                barrier: ToolPlanBarrier::default(),
            },
        },
        budget_plan: DecisionBudgetEstimate {
            tokens: Some(512),
            walltime_ms: Some(800),
            external_cost: Some(1.5),
        },
        rationale: DecisionRationale::default(),
        confidence: 0.72,
        explain: DecisionExplain {
            routing_seed: 42,
            router_digest: "blake3:route".into(),
            router_config_digest: "blake3:config".into(),
            features_snapshot: None,
        },
        degradation_reason: Some(AwarenessDegradationReason::GraphDegraded),
    };

    let decision = RouterDecision {
        plan: route_plan,
        decision_path,
        rejected: Vec::new(),
        context_digest: "sha256:context".into(),
        issued_at: OffsetDateTime::now_utc(),
    };

    let input = build_llm_input(
        &decision,
        LlmIntegrationOptions::new("请总结工具执行".into())
            .context_tags(json!({"scene": "router"}))
            .clarify_prompt(Some("请输出最终答案".into()))
            .tool_summary(None)
            .tool_indices(Some(vec!["tool_idx".into()]))
            .tool_query_hash(Some("tool#hash".into())),
    )
    .expect("build llm input");

    assert_eq!(input.anchor.tenant_id, awareness_anchor.tenant_id);
    assert_eq!(input.scene, "tool_llm");
    assert_eq!(
        input.tool_indices.as_ref().expect("tool indices"),
        &vec![String::from("timeline_v2"), String::from("tool_idx")]
    );
    assert_eq!(
        input.tool_query_hash.as_deref().expect("query hash"),
        "dfr#route|tool#hash"
    );
    assert_eq!(input.degrade_hint.as_deref(), Some("graph_degraded"));
    assert_eq!(input.user_prompt, "请总结工具执行");
    assert!(input.context_tags.is_object());
}
#[test]
fn build_llm_input_scene_variants() {
    let awareness_anchor = AwarenessAnchor {
        tenant_id: TenantId::new(88),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg:dfr".into(),
        config_snapshot_version: 3,
        session_id: Some(SessionId::new(456)),
        sequence_number: Some(5),
        access_class: AccessClass::Internal,
        provenance: Some(Provenance {
            source: "router".into(),
            method: "ace".into(),
            model: Some("planner".into()),
            content_digest_sha256: None,
        }),
        schema_v: 1,
    };

    let make_plan = |fork: AwarenessFork,
                     explain_degrade: Option<&str>,
                     path_degrade: Option<AwarenessDegradationReason>| {
        RouterDecision {
            plan: RoutePlan {
                cycle_id: soulseed_agi_core_models::CycleId(10),
                anchor: awareness_anchor.clone(),
                fork,
                decision_plan: match fork {
                    AwarenessFork::Clarify => AwarenessDecisionPlan::Clarify {
                        plan: ClarifyPlan {
                            questions: Vec::new(),
                            limits: ClarifyLimits::default(),
                        },
                    },
                    AwarenessFork::SelfReason => AwarenessDecisionPlan::SelfReason {
                        plan: SelfPlan {
                            hint: None,
                            max_ic: Some(3),
                        },
                    },
                    AwarenessFork::ToolPath => AwarenessDecisionPlan::Tool {
                        plan: ToolPlan {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            barrier: ToolPlanBarrier::default(),
                        },
                    },
                    AwarenessFork::Collab => AwarenessDecisionPlan::Collab {
                        plan: CollabPlan {
                            scope: json!({"channel": "team"}),
                            order: Some("round_robin".into()),
                            rounds: Some(1),
                            privacy_mode: Some("shared".into()),
                            barrier: ToolPlanBarrier::default(),
                        },
                    },
                },
                budget: BudgetEstimate {
                    tokens: 256,
                    walltime_ms: 400,
                    external_cost: 0.6,
                },
                priority: 0.8,
                explain: RouteExplain {
                    routing_seed: 10,
                    router_digest: "blake3:route".into(),
                    router_config_digest: "blake3:cfg".into(),
                    indices_used: vec!["route_idx".into()],
                    query_hash: Some("route_hash".into()),
                    degradation_reason: explain_degrade.map(|s| s.to_string()),
                    diagnostics: json!({"fork": format!("{:?}", fork)}),
                    rejected: Vec::new(),
                },
            },
            decision_path: AwarenessDecisionPath {
                anchor: awareness_anchor.clone(),
                awareness_cycle_id: soulseed_agi_core_models::CycleId(10),
                inference_cycle_sequence: 1,
                fork,
                plan: match fork {
                    AwarenessFork::Clarify => AwarenessDecisionPlan::Clarify {
                        plan: ClarifyPlan {
                            questions: Vec::new(),
                            limits: ClarifyLimits::default(),
                        },
                    },
                    AwarenessFork::SelfReason => AwarenessDecisionPlan::SelfReason {
                        plan: SelfPlan {
                            hint: None,
                            max_ic: Some(3),
                        },
                    },
                    AwarenessFork::ToolPath => AwarenessDecisionPlan::Tool {
                        plan: ToolPlan {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            barrier: ToolPlanBarrier::default(),
                        },
                    },
                    AwarenessFork::Collab => AwarenessDecisionPlan::Collab {
                        plan: CollabPlan {
                            scope: json!({"channel": "team"}),
                            order: Some("round_robin".into()),
                            rounds: Some(1),
                            privacy_mode: Some("shared".into()),
                            barrier: ToolPlanBarrier::default(),
                        },
                    },
                },
                budget_plan: DecisionBudgetEstimate::default(),
                rationale: DecisionRationale::default(),
                confidence: 0.67,
                explain: DecisionExplain {
                    routing_seed: 10,
                    router_digest: "blake3:route".into(),
                    router_config_digest: "blake3:cfg".into(),
                    features_snapshot: None,
                },
                degradation_reason: path_degrade,
            },
            rejected: Vec::new(),
            context_digest: "sha256:ctx".into(),
            issued_at: OffsetDateTime::now_utc(),
        }
    };

    let clarify_input = build_llm_input(
        &make_plan(
            AwarenessFork::Clarify,
            Some("clarify_pending"),
            Some(AwarenessDegradationReason::ClarifyExhausted),
        ),
        LlmIntegrationOptions::new("clarify answer".into())
            .clarify_prompt(Some("请确认问题".into()))
            .context_tags(json!({"step": "clarify"})),
    )
    .expect("clarify input");
    assert_eq!(clarify_input.scene, "clarify_llm");
    assert_eq!(
        clarify_input.degrade_hint.as_deref(),
        Some("clarify_pending")
    );

    let self_input = build_llm_input(
        &make_plan(
            AwarenessFork::SelfReason,
            None,
            Some(AwarenessDegradationReason::EnvctxDegraded),
        ),
        LlmIntegrationOptions::new("self reflection".into())
            .context_tags(json!({"mode": "self"}))
            .tool_indices(Some(vec!["ctx_idx".into()]))
            .tool_query_hash(Some("ctx_hash".into())),
    )
    .expect("self input");
    assert_eq!(self_input.scene, "self_reason_llm");
    assert_eq!(self_input.degrade_hint.as_deref(), Some("envctx_degraded"));
    assert_eq!(
        self_input.tool_indices.as_ref().expect("self indices"),
        &vec![String::from("route_idx"), String::from("ctx_idx")]
    );
    assert_eq!(
        self_input.tool_query_hash.as_deref().expect("query hash"),
        "route_hash|ctx_hash"
    );

    let collab_input = build_llm_input(
        &make_plan(
            AwarenessFork::Collab,
            None,
            Some(AwarenessDegradationReason::PrivacyBlocked),
        ),
        LlmIntegrationOptions::new("collab summary".into()).context_tags(json!({"mode": "collab"})),
    )
    .expect("collab input");
    assert_eq!(collab_input.scene, "collab_llm");
    assert_eq!(
        collab_input.degrade_hint.as_deref(),
        Some("privacy_blocked")
    );
}

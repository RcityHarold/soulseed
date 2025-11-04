use std::collections::HashSet;

use soulseed_agi_graph::{
    api::{
        AwarenessFilters, AwarenessQuery, CausalDir, CausalQuery, ExplainReplayQuery,
        InfluenceQuery, NeighborhoodDirection, NeighborhoodQuery, PathQuery, PathStrategy,
        PatternQuery, RecallFilters, RecallQuery, RecallQueryTextOrVec, SimilarityQuery,
        SimilarityStrategy, SubgraphQuery, TimelineQuery, EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::GraphError,
    mock::{Capabilities, MockExecutor},
    plan::{Plan, PlannerConfig},
    planner::Planner,
    scenario_rule, AccessClass, AwarenessCycleId, AwarenessDegradationReason, AwarenessEventType,
    AwarenessFork, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventPayloadKind,
    DialogueEventType, EnvelopeHead, EventId, HumanId, InvariantCheck, LiveFilters, LiveSubscribe,
    MessageId, MessagePointer, SemanticEdgeKind, SessionId, Snapshot, Subject, SyncPointKind,
    TenantId, TraceId,
};

fn planner_env(vector_available: bool) -> (Planner, MockExecutor) {
    let planner = Planner::new(PlannerConfig::default());
    let caps = Capabilities {
        allowed_tenants: [1u64].into_iter().collect::<HashSet<_>>(),
        vector_idx_available: vector_available,
        live_max_per_tenant: 10,
    };
    let exec = MockExecutor::new(caps);
    (planner, exec)
}

fn sample_event() -> DialogueEvent {
    DialogueEvent {
        tenant_id: TenantId::new(1),
        event_id: EventId::new(1),
        session_id: SessionId::new(1),
        subject: Subject::Human(HumanId::new(42)),
        participants: vec![],
        head: EnvelopeHead {
            envelope_id: uuid::Uuid::now_v7(),
            trace_id: TraceId("trc".into()),
            correlation_id: CorrelationId("corr".into()),
            config_snapshot_hash: "hash".into(),
            config_snapshot_version: 1,
        },
        snapshot: Snapshot {
            schema_v: 1,
            created_at: time::OffsetDateTime::now_utc(),
        },
        timestamp_ms: 1,
        scenario: ConversationScenario::HumanToAi,
        event_type: DialogueEventType::Message,
        sequence_number: 1,
        access_class: AccessClass::Public,
        provenance: None,
        time_window: None,
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: Vec::new(),
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
        real_time_priority: None,
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
        message_ref: Some(MessagePointer {
            message_id: MessageId::new(99),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: serde_json::json!({}),
        #[cfg(feature = "vectors-extra")]
        vectors: soulseed_agi_core_models::ExtraVectors::default(),
    }
}

fn awareness_query_base() -> AwarenessQuery {
    AwarenessQuery {
        tenant_id: TenantId::new(1),
        filters: AwarenessFilters {
            awareness_cycle_id: Some(AwarenessCycleId::new(777)),
            parent_cycle_id: None,
            collab_scope_id: None,
            barrier_id: None,
            env_mode: None,
            inference_cycle_sequence: Some(1),
            event_types: Some(vec![
                AwarenessEventType::AwarenessCycleStarted,
                AwarenessEventType::InferenceCycleCompleted,
            ]),
            degradation_reasons: None,
            sync_point_kinds: None,
        },
        limit: 50,
        after: None,
    }
}

fn explain_query_base() -> ExplainReplayQuery {
    ExplainReplayQuery {
        tenant_id: TenantId::new(1),
        awareness_cycle_id: AwarenessCycleId::new(777),
        forks: None,
        event_types: None,
        limit: 100,
        after: None,
    }
}

#[test]
fn scenario_rule_includes_payloads_and_routes() {
    let rule = scenario_rule(&ConversationScenario::HumanToAi);
    assert!(rule
        .primary_payloads
        .contains(&DialogueEventPayloadKind::MessagePrimary));
    assert!(rule.allowed_forks.contains(&AwarenessFork::Clarify));
    assert!(rule
        .explain_fields
        .contains(&"decision.explain.router_digest"));
}

#[test]
fn timeline_requires_indices_and_tenant_check() {
    let (planner, exec) = planner_env(true);
    let query = TimelineQuery {
        tenant_id: TenantId::new(1),
        session_id: Some(SessionId::new(10)),
        participants: None,
        window: None,
        after: None,
        limit: 20,
        require_fields: None,
        scenario: None,
        event_types: None,
    };
    let (plan, _, _) = planner.plan_timeline(&query).unwrap();
    let resp = exec.execute_timeline(plan, &query).unwrap();
    assert!(!resp.indices_used.is_empty());
}

#[test]
fn causal_depth_truncates_and_exposes_reason() {
    let (planner, exec) = planner_env(true);
    let query = CausalQuery {
        tenant_id: TenantId::new(1),
        root_event: EventId::new(5),
        direction: CausalDir::Both,
        max_depth: Some(5),
        time_window: None,
        scenario: None,
        event_types: None,
    };
    let (plan, _, degr) = planner.plan_causal(&query).unwrap();
    let resp = exec.execute_causal(plan, &query, degr.clone()).unwrap();
    assert_eq!(resp.degradation_reason, degr.map(|d| d.reason));
    assert!(!resp.concept_nodes.is_empty());
    assert!(!resp.semantic_edges.is_empty());
    assert_eq!(
        resp.semantic_edges[0].kind,
        SemanticEdgeKind::EventToConcept
    );
}

#[test]
fn recall_falls_back_to_sparse_when_vector_missing() {
    let planner = Planner::new(PlannerConfig::default());
    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: false,
        live_max_per_tenant: 10,
    });
    let query = RecallQuery {
        tenant_id: TenantId::new(1),
        query: RecallQueryTextOrVec::Vec(vec![0.1; 4]),
        k: 10,
        filters: RecallFilters {
            scenes: None,
            topics: None,
            time_window: None,
            participant: None,
            event_types: None,
        },
    };
    let (plan, _, degr) = planner
        .plan_recall(&query, exec.caps.vector_idx_available, 4)
        .unwrap();
    let resp = exec.execute_recall(plan, &query, degr.clone()).unwrap();
    assert_eq!(resp.degradation_reason, degr.map(|d| d.reason));
}

#[test]
fn timeline_filters_reject_mismatched_scenario() {
    let (planner, exec) = planner_env(true);
    let query = TimelineQuery {
        tenant_id: TenantId::new(1),
        session_id: Some(SessionId::new(10)),
        participants: None,
        window: None,
        after: None,
        limit: 20,
        require_fields: None,
        scenario: Some(ConversationScenario::HumanToAi),
        event_types: Some(vec![DialogueEventType::Message]),
    };
    let (plan, _, _) = planner.plan_timeline(&query).unwrap();
    let mut altered = query.clone();
    altered.scenario = Some(ConversationScenario::AiGroup);
    let err = exec.execute_timeline(plan, &altered).unwrap_err();
    assert!(matches!(err, GraphError::InvalidQuery(_)));
}

#[test]
fn recall_filters_enforce_event_types() {
    let planner = Planner::new(PlannerConfig::default());
    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    let query = RecallQuery {
        tenant_id: TenantId::new(1),
        query: RecallQueryTextOrVec::Text("test".into()),
        k: 5,
        filters: RecallFilters {
            scenes: None,
            topics: None,
            time_window: None,
            participant: None,
            event_types: Some(vec![DialogueEventType::ToolResult]),
        },
    };
    let (plan, _, _) = planner
        .plan_recall(&query, exec.caps.vector_idx_available, 768)
        .unwrap();
    let mut altered = query;
    altered.filters.event_types = Some(vec![DialogueEventType::SelfReflection]);
    let err = exec.execute_recall(plan, &altered, None).unwrap_err();
    assert!(matches!(err, GraphError::InvalidQuery(_)));
}

#[test]
fn restricted_event_without_provenance_fails_validation() {
    let mut evt = sample_event();
    evt.access_class = AccessClass::Restricted;
    let err = InvariantCheck::validate(&evt).unwrap_err();
    assert_eq!(err, GraphError::PrivacyRestricted);
}

#[test]
fn session_sequence_conflict_detected() {
    let mut exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    let mut evt1 = sample_event();
    evt1.sequence_number = 1;
    let mut evt2 = sample_event();
    evt2.event_id = EventId::new(2);
    evt2.sequence_number = 1;
    exec.append_event(evt1).unwrap();
    let err = exec.append_event(evt2).unwrap_err();
    assert_eq!(err, GraphError::StorageConflict);
}

#[test]
fn tenant_mismatch_is_forbidden() {
    let planner = Planner::new(PlannerConfig::default());
    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [2u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    let query = TimelineQuery {
        tenant_id: TenantId::new(1),
        session_id: Some(SessionId::new(10)),
        participants: None,
        window: None,
        after: None,
        limit: 10,
        require_fields: None,
        scenario: None,
        event_types: None,
    };
    let (plan, _, _) = planner.plan_timeline(&query).unwrap();
    let err = exec.execute_timeline(plan, &query).unwrap_err();
    assert_eq!(err, GraphError::AuthForbidden);
}

#[test]
fn live_plan_clamps_rate_and_validates_scene() {
    let planner = Planner::new(PlannerConfig::default());
    let subscribe = LiveSubscribe {
        tenant_id: TenantId::new(1),
        filters: LiveFilters {
            session: Some(SessionId::new(99)),
            participants: None,
            scene: Some(ConversationScenario::HumanToAi),
            priority: Some("high".into()),
        },
        max_rate: Some(999),
        heartbeat_ms: Some(200),
        idle_timeout_ms: None,
        max_buffer: Some(800),
        backpressure_mode: Some("drop_tail".into()),
    };
    let (plan, _, degr) = planner.plan_live(&subscribe).unwrap();
    if let Plan::Live { rate, .. } = &plan.plan {
        assert_eq!(*rate, 60);
    } else {
        panic!("expected live plan");
    }
    assert_eq!(degr.map(|d| d.reason), Some("rate_clamped".into()));

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 60,
    });
    exec.execute_live(plan, &subscribe).unwrap();

    let (plan_mismatch, _, _) = planner.plan_live(&subscribe).unwrap();
    let mut mismatch = subscribe.clone();
    mismatch.filters.scene = Some(ConversationScenario::AiGroup);
    let err = exec.execute_live(plan_mismatch, &mismatch).unwrap_err();
    assert!(matches!(err, GraphError::InvalidQuery(_)));
}

#[test]
fn scenario_rule_provides_expected_event_types() {
    let rule = scenario_rule(&ConversationScenario::HumanToAi);
    assert!(rule
        .primary_event_types
        .iter()
        .any(|ty| matches!(ty, DialogueEventType::Message)));
    assert!(rule
        .allowed_semantic_edges
        .iter()
        .any(|edge| matches!(edge, SemanticEdgeKind::EventToConcept)));
}

#[test]
fn awareness_cycle_query_uses_cycle_index() {
    let planner = Planner::new(PlannerConfig::default());
    let query = awareness_query_base();
    let (plan, hash, degr) = planner.plan_awareness(&query).unwrap();
    assert!(hash.starts_with("awareness:tenant=1"));
    assert!(degr.is_none());
    assert_eq!(
        plan.indices_used,
        vec!["ix_awareness_cycle_time".to_string()]
    );

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    exec.execute_awareness(plan, &query).unwrap();
}

#[test]
fn awareness_barrier_prefers_barrier_index() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = awareness_query_base();
    query.filters.barrier_id = Some("bar-9".into());
    let (plan, _, _) = planner.plan_awareness(&query).unwrap();
    assert_eq!(
        plan.indices_used,
        vec![
            "ix_awareness_barrier".to_string(),
            "ix_awareness_cycle_time".to_string()
        ]
    );

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    exec.execute_awareness(plan, &query).unwrap();
}

#[test]
fn awareness_parent_scope_mismatch_rejected() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = awareness_query_base();
    query.filters.collab_scope_id = Some("scope-A".into());
    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    let (plan_ok, _, _) = planner.plan_awareness(&query).unwrap();
    exec.execute_awareness(plan_ok, &query).unwrap();

    let (plan, _, _) = planner.plan_awareness(&query).unwrap();
    let mut mismatched = query.clone();
    mismatched.filters.collab_scope_id = Some("scope-B".into());
    let err = exec.execute_awareness(plan, &mismatched).unwrap_err();
    assert!(matches!(
        err,
        GraphError::InvalidQuery("collab_scope_mismatch")
    ));
}

#[test]
fn awareness_hitl_filters_require_subset() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = awareness_query_base();
    query.filters.event_types = Some(vec![AwarenessEventType::HumanInjectionApplied]);
    query.filters.degradation_reasons = Some(vec![AwarenessDegradationReason::ClarifyExhausted]);
    query.filters.sync_point_kinds = Some(vec![SyncPointKind::HitlAbsorb]);
    let (plan, _, _) = planner.plan_awareness(&query).unwrap();

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    exec.execute_awareness(plan, &query).unwrap();

    let (plan_mismatch, _, _) = planner.plan_awareness(&query).unwrap();
    let mut mismatch = query.clone();
    mismatch.filters.degradation_reasons = Some(vec![AwarenessDegradationReason::BudgetTokens]);
    let err = exec
        .execute_awareness(plan_mismatch, &mismatch)
        .unwrap_err();
    assert!(matches!(
        err,
        GraphError::InvalidQuery("degradation_mismatch")
    ));
}

#[test]
fn awareness_late_receipt_filters_enforced() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = awareness_query_base();
    query.filters.event_types = Some(vec![AwarenessEventType::LateReceiptObserved]);
    query.filters.degradation_reasons = Some(vec![AwarenessDegradationReason::GraphDegraded]);
    let (plan, _, _) = planner.plan_awareness(&query).unwrap();

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 10,
    });
    exec.execute_awareness(plan.clone(), &query).unwrap();

    let mut mismatch = query.clone();
    mismatch.filters.event_types = Some(vec![AwarenessEventType::Finalized]);
    let err = exec.execute_awareness(plan, &mismatch).unwrap_err();
    assert!(matches!(
        err,
        GraphError::InvalidQuery("event_type_mismatch")
    ));
}

#[test]
fn explain_replay_defaults_use_awareness_plan() {
    let planner = Planner::new(PlannerConfig::default());
    let query = explain_query_base();
    let (prepared, hash, degradation) = planner.plan_explain_replay(&query).unwrap();
    assert!(hash.contains("explain:"));
    assert!(degradation.is_none());

    if let Plan::Awareness { filters, .. } = &prepared.plan {
        let events = filters.event_types.as_ref().expect("event types present");
        assert_eq!(events, &EXPLAIN_EVENT_TYPES_DEFAULT.to_vec());
        let sync = filters.sync_point_kinds.as_ref().expect("sync kinds set");
        assert!(sync.contains(&SyncPointKind::ClarifyAnswered));
        assert!(sync.contains(&SyncPointKind::ToolBarrier));
    } else {
        panic!("expected awareness plan");
    }

    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 5,
    });
    exec.execute_explain_replay(prepared, &query, None).unwrap();
}

#[test]
fn explain_replay_forks_return_degradation() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = explain_query_base();
    query.forks = Some(vec![AwarenessFork::Clarify]);
    let (_plan, _hash, degradation) = planner.plan_explain_replay(&query).unwrap();
    let reason = degradation.expect("expected degradation").reason;
    assert_eq!(reason, "fork_filter_not_supported");
}

#[test]
fn explain_replay_executor_surfaces_degradation_reason() {
    let planner = Planner::new(PlannerConfig::default());
    let mut query = explain_query_base();
    query.forks = Some(vec![AwarenessFork::ToolPath]);
    let (plan, _, degradation) = planner.plan_explain_replay(&query).unwrap();
    let exec = MockExecutor::new(Capabilities {
        allowed_tenants: [1u64].into_iter().collect(),
        vector_idx_available: true,
        live_max_per_tenant: 5,
    });
    let resp = exec
        .execute_explain_replay(plan, &query, degradation)
        .unwrap();
    assert_eq!(
        resp.degradation_reason.as_deref(),
        Some("fork_filter_not_supported")
    );
    assert!(resp.segments.is_empty());
}

#[test]
fn path_planner_truncates_depth_and_paths() {
    let cfg = PlannerConfig::default();
    let planner = Planner::new(cfg.clone());
    let query = PathQuery {
        tenant_id: TenantId::new(1),
        start: EventId::new(10),
        goal: EventId::new(20),
        strategy: PathStrategy::BidirectionalDijkstra,
        max_depth: Some(cfg.max_path_depth + 5),
        max_paths: (cfg.max_limit as u16) + 50,
        time_window: None,
        scenario: None,
    };
    let (prepared, hash, degradation) = planner.plan_path(&query).unwrap();
    assert!(hash.starts_with("path:tenant="));
    let reason = degradation.expect("expected degradation").reason;
    assert!(reason.contains("depth_truncated"));
    assert!(reason.contains("path_count_truncated"));
    if let Plan::Path {
        max_depth,
        max_paths,
        ..
    } = prepared.plan
    {
        assert_eq!(max_depth, cfg.max_path_depth);
        assert_eq!(max_paths, cfg.max_limit as u16);
    } else {
        panic!("expected path plan");
    }
}

#[test]
fn neighborhood_planner_respects_radius_limit() {
    let cfg = PlannerConfig::default();
    let planner = Planner::new(cfg.clone());
    let query = NeighborhoodQuery {
        tenant_id: TenantId::new(1),
        center: EventId::new(42),
        direction: NeighborhoodDirection::Both,
        radius: cfg.max_neighborhood_radius + 2,
        limit: cfg.max_limit + 100,
        scenario: None,
    };
    let (prepared, _, degradation) = planner.plan_neighborhood(&query).unwrap();
    let reason = degradation.expect("expected degradation").reason;
    assert!(reason.contains("radius_truncated"));
    assert!(reason.contains("limit_truncated"));
    if let Plan::Neighborhood { radius, limit, .. } = prepared.plan {
        assert_eq!(radius, cfg.max_neighborhood_radius);
        assert_eq!(limit, cfg.max_limit);
    } else {
        panic!("expected neighborhood plan");
    }
}

#[test]
fn subgraph_planner_requires_seeds() {
    let planner = Planner::new(PlannerConfig::default());
    let query = SubgraphQuery {
        tenant_id: TenantId::new(1),
        seeds: Vec::new(),
        radius: 1,
        max_nodes: 10,
        scenario: None,
        include_artifacts: Some(false),
    };
    let err = planner.plan_subgraph(&query).unwrap_err();
    assert_eq!(err, GraphError::InvalidQuery("seeds_required"));
}

#[test]
fn similarity_planner_falls_back_without_vector_index() {
    let planner = Planner::new(PlannerConfig::default());
    let query = SimilarityQuery {
        tenant_id: TenantId::new(1),
        anchor_event: EventId::new(9),
        top_k: 300,
        strategy: SimilarityStrategy::Vector,
        filters: RecallFilters {
            scenes: None,
            topics: None,
            time_window: None,
            participant: None,
            event_types: None,
        },
    };
    let (_plan, _, degradation) = planner
        .plan_similarity(&query, false)
        .expect("planner should succeed");
    let reason = degradation.expect("expected degradation").reason;
    assert!(reason.contains("vector_index_unavailable"));
    assert!(reason.contains("similarity_k_truncated"));
}

#[test]
fn influence_planner_clamps_iterations_and_damping() {
    let cfg = PlannerConfig::default();
    let planner = Planner::new(cfg.clone());
    let query = InfluenceQuery {
        tenant_id: TenantId::new(1),
        seeds: vec![EventId::new(1), EventId::new(2)],
        horizon_ms: Some(10_000),
        damping_factor: 1.5,
        iterations: cfg.max_influence_iterations + 20,
        scenario: None,
    };
    let (prepared, _, degradation) = planner.plan_influence(&query).unwrap();
    let reason = degradation.expect("expected degradation").reason;
    assert!(reason.contains("iterations_truncated"));
    assert!(reason.contains("damping_clamped"));
    if let Plan::Influence {
        iterations,
        damping_factor,
        ..
    } = prepared.plan
    {
        assert_eq!(iterations, cfg.max_influence_iterations);
        assert!((damping_factor - 1.0).abs() < f32::EPSILON);
    } else {
        panic!("expected influence plan");
    }
}

#[test]
fn pattern_planner_limits_results() {
    let cfg = PlannerConfig::default();
    let planner = Planner::new(cfg.clone());
    let query = PatternQuery {
        tenant_id: TenantId::new(1),
        template_id: "test-pattern".into(),
        limit: cfg.max_pattern_limit + 500,
        parameters: serde_json::json!({ "threshold": 0.8 }),
    };
    let (prepared, _, degradation) = planner.plan_pattern(&query).unwrap();
    assert_eq!(
        degradation.expect("expected degradation").reason,
        "limit_truncated"
    );
    if let Plan::Pattern { limit, .. } = prepared.plan {
        assert_eq!(limit, cfg.max_pattern_limit);
    } else {
        panic!("expected pattern plan");
    }
}

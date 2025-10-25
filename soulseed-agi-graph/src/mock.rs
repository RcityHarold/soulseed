use std::collections::{HashMap, HashSet};

use crate::{
    api::{
        AwarenessFilters, AwarenessQuery, AwarenessResponse, CausalEdge, ExplainReplayQuery,
        ExplainReplayResponse, ExplainReplaySegment, GraphView, InfluenceQuery, InfluenceResponse,
        NeighborhoodQuery, NeighborhoodResponse, PathQuery, PathResponse, PathStrategy,
        PatternQuery, PatternResponse, RecallResponse, SimilarityQuery, SimilarityResponse,
        SubgraphQuery, SubgraphResponse, EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::{Degradation, GraphError},
    plan::PreparedPlan,
    scenario::scenario_rule,
    types::{
        ConceptNode, EmotionNode, GraphEdge, GraphEdgeKind, GraphNode, GraphNodeKind, GraphNodeRef,
        SemanticEdge, SemanticEdgeKind, SemanticRef, TopicNode,
    },
    AwarenessEventType, DialogueEvent, EventId, LiveEventPointer, LiveSubscribe, RecallQuery,
    SyncPointKind, TimelineQuery, TimelineResponse,
};
use serde_json::json;
use soulseed_agi_core_models::validate_dialogue_event;

#[derive(Clone, Debug)]
pub struct Capabilities {
    pub allowed_tenants: HashSet<u64>,
    pub vector_idx_available: bool,
    pub live_max_per_tenant: usize,
}

pub struct MockExecutor {
    pub caps: Capabilities,
    pub timeline_store: HashMap<(u64, u64, u64), DialogueEvent>,
}

impl MockExecutor {
    pub fn new(caps: Capabilities) -> Self {
        Self {
            caps,
            timeline_store: HashMap::new(),
        }
    }

    pub fn append_event(&mut self, event: DialogueEvent) -> Result<(), GraphError> {
        if let Err(_err) = validate_dialogue_event(&event) {
            return Err(GraphError::InvalidQuery("invalid_event_structure"));
        }
        if !self
            .caps
            .allowed_tenants
            .contains(&event.base.tenant_id.as_u64())
        {
            return Err(GraphError::AuthForbidden);
        }
        let key = (
            event.base.tenant_id.as_u64(),
            event.base.session_id.as_u64(),
            event.base.sequence_number,
        );
        if self.timeline_store.contains_key(&key) {
            return Err(GraphError::StorageConflict);
        }
        self.timeline_store.insert(key, event);
        Ok(())
    }

    pub fn execute_timeline(
        &self,
        plan: PreparedPlan,
        query: &TimelineQuery,
    ) -> Result<TimelineResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        match &plan.plan {
            crate::plan::Plan::Timeline {
                scenario,
                event_types,
                ..
            } => {
                if let Some(expected) = scenario {
                    if let Some(actual) = query.scenario.as_ref() {
                        if actual != expected {
                            return Err(GraphError::InvalidQuery("scenario_mismatch"));
                        }
                    }
                    if let Some(types) = event_types {
                        let rule = scenario_rule(expected);
                        if !rule.primary_event_types.iter().all(|ty| types.contains(ty)) {
                            return Err(GraphError::InvalidQuery("scenario_event_type_mismatch"));
                        }
                    }
                }
                if let Some(expected) = event_types {
                    if let Some(actual) = query.event_types.as_ref() {
                        if !actual.iter().all(|ty| expected.contains(ty)) {
                            return Err(GraphError::InvalidQuery("event_type_mismatch"));
                        }
                    }
                }
            }
            crate::plan::Plan::SessionOrder { scenario, .. } => {
                if let Some(expected) = scenario {
                    if let Some(actual) = query.scenario.as_ref() {
                        if actual != expected {
                            return Err(GraphError::InvalidQuery("scenario_mismatch"));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(TimelineResponse {
            tenant_id: query.tenant_id,
            items: Vec::new(),
            next: None,
            indices_used: plan.indices_used,
            query_hash: "mock-timeline".into(),
            degradation_reason: None,
            graph: None,
        })
    }

    pub fn execute_causal(
        &self,
        plan: PreparedPlan,
        query: &crate::api::CausalQuery,
        degradation: Option<Degradation>,
    ) -> Result<crate::api::CausalResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        if let crate::plan::Plan::Causal {
            scenario,
            event_types,
            ..
        } = &plan.plan
        {
            if let Some(expected) = scenario {
                if let Some(actual) = query.scenario.as_ref() {
                    if actual != expected {
                        return Err(GraphError::InvalidQuery("scenario_mismatch"));
                    }
                }
                if let Some(types) = event_types {
                    let rule = scenario_rule(expected);
                    if !rule.primary_event_types.iter().all(|ty| types.contains(ty)) {
                        return Err(GraphError::InvalidQuery("scenario_event_type_mismatch"));
                    }
                }
            }
            if let Some(expected) = event_types {
                if let Some(actual) = query.event_types.as_ref() {
                    if !actual.iter().all(|ty| expected.contains(ty)) {
                        return Err(GraphError::InvalidQuery("event_type_mismatch"));
                    }
                }
            }
        }
        let plan_scenario = match &plan.plan {
            crate::plan::Plan::Causal { scenario, .. } => scenario.clone(),
            _ => None,
        };
        let semantic_edges = vec![SemanticEdge {
            from: SemanticRef::Event(query.root_event),
            to: SemanticRef::Concept(format!("concept:{}", query.root_event.as_u64())),
            kind: SemanticEdgeKind::EventToConcept,
            weight: Some(0.7),
        }];

        if let Some(scene) = query.scenario.as_ref().or(plan_scenario.as_ref()) {
            let allowed = scenario_rule(scene).allowed_semantic_edges;
            if semantic_edges
                .iter()
                .any(|edge| !allowed.contains(&edge.kind))
            {
                return Err(GraphError::InvalidQuery("semantic_edge_not_allowed"));
            }
        }

        Ok(crate::api::CausalResponse {
            tenant_id: query.tenant_id,
            nodes: Vec::new(),
            edges: vec![CausalEdge {
                from: query.root_event,
                to: EventId::from_raw_unchecked(query.root_event.as_u64() + 1),
                edge_type: "TRIGGERED".into(),
                weight: Some(0.7),
            }],
            concept_nodes: vec![ConceptNode {
                concept_id: format!("concept:{}", query.root_event.as_u64()),
                label: "goal_alignment".into(),
                score: Some(0.82),
                evidence_pointer: None,
            }],
            topic_nodes: vec![TopicNode {
                topic_id: "topic:collaboration".into(),
                name: "collaboration".into(),
                salience: Some(0.64),
                keywords: Some(vec!["plan".into(), "tool".into()]),
            }],
            emotion_nodes: vec![EmotionNode {
                emotion: "curiosity".into(),
                intensity: Some(0.4),
                evidence_pointer: None,
            }],
            semantic_edges,
            graph_nodes: Vec::new(),
            graph_edges: Vec::new(),
            indices_used: plan.indices_used,
            query_hash: "mock-causal".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_path(
        &self,
        plan: PreparedPlan,
        query: &PathQuery,
        degradation: Option<Degradation>,
    ) -> Result<PathResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        let (strategy, max_depth, max_paths) = match &plan.plan {
            crate::plan::Plan::Path {
                strategy,
                max_depth,
                max_paths,
                ..
            } => (strategy.clone(), *max_depth, *max_paths),
            _ => return Err(GraphError::NoIndexPlan),
        };
        let mut nodes = Vec::new();
        nodes.push(GraphNode {
            id: GraphNodeRef::Event(query.start),
            kind: GraphNodeKind::Event,
            label: Some("start".into()),
            summary: None,
            weight: Some(1.0),
            metadata: None,
        });
        nodes.push(GraphNode {
            id: GraphNodeRef::Event(query.goal),
            kind: GraphNodeKind::Event,
            label: Some("goal".into()),
            summary: None,
            weight: Some(1.0),
            metadata: None,
        });
        let edges = vec![GraphEdge {
            from: GraphNodeRef::Event(query.start),
            to: GraphNodeRef::Event(query.goal),
            kind: GraphEdgeKind::RoutesThrough,
            strength: Some(0.6),
            confidence: Some(0.7),
            temporal_decay: Some(0.1),
            since_ms: None,
            until_ms: None,
            explain: Some("mock_path_edge".into()),
            properties: json!({
                "max_depth": max_depth,
                "max_paths": max_paths
            }),
        }];
        let graph = GraphView {
            nodes,
            edges,
            summary: Some("mock_path".into()),
        };
        let algorithm = match strategy.as_str() {
            "bidirectional_dijkstra" => PathStrategy::BidirectionalDijkstra,
            _ => PathStrategy::Bfs,
        };
        Ok(PathResponse {
            tenant_id: query.tenant_id,
            algorithm,
            hop_count: 1,
            total_cost: None,
            graph,
            indices_used: plan.indices_used,
            query_hash: "mock-path".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_neighborhood(
        &self,
        plan: PreparedPlan,
        query: &NeighborhoodQuery,
        degradation: Option<Degradation>,
    ) -> Result<NeighborhoodResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        let radius = match &plan.plan {
            crate::plan::Plan::Neighborhood { radius, .. } => *radius,
            _ => return Err(GraphError::NoIndexPlan),
        };
        let nodes = vec![GraphNode {
            id: GraphNodeRef::Event(query.center),
            kind: GraphNodeKind::Event,
            label: Some("center".into()),
            summary: None,
            weight: Some(1.0),
            metadata: None,
        }];
        let edges = Vec::new();
        let graph = GraphView {
            nodes,
            edges,
            summary: Some("mock_neighborhood".into()),
        };
        Ok(NeighborhoodResponse {
            tenant_id: query.tenant_id,
            graph,
            expansion_depth: radius,
            indices_used: plan.indices_used,
            query_hash: "mock-neighborhood".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_subgraph(
        &self,
        plan: PreparedPlan,
        query: &SubgraphQuery,
        degradation: Option<Degradation>,
    ) -> Result<SubgraphResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        if query.seeds.is_empty() {
            return Err(GraphError::InvalidQuery("seeds_required"));
        }
        let (radius, include_artifacts) = match &plan.plan {
            crate::plan::Plan::Subgraph {
                radius,
                include_artifacts,
                ..
            } => (*radius, *include_artifacts),
            _ => return Err(GraphError::NoIndexPlan),
        };
        let nodes: Vec<GraphNode> = query
            .seeds
            .iter()
            .map(|seed| GraphNode {
                id: GraphNodeRef::Event(*seed),
                kind: GraphNodeKind::Event,
                label: Some(format!("seed:{}", seed.as_u64())),
                summary: None,
                weight: Some(0.9),
                metadata: Some(json!({ "radius": radius })),
            })
            .collect();
        let graph = GraphView {
            nodes,
            edges: Vec::new(),
            summary: Some(if include_artifacts {
                "mock_subgraph_with_artifacts".into()
            } else {
                "mock_subgraph".into()
            }),
        };
        Ok(SubgraphResponse {
            tenant_id: query.tenant_id,
            graph,
            indices_used: plan.indices_used,
            query_hash: "mock-subgraph".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_similarity(
        &self,
        plan: PreparedPlan,
        query: &SimilarityQuery,
        degradation: Option<Degradation>,
    ) -> Result<SimilarityResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        let top_k = match &plan.plan {
            crate::plan::Plan::Similarity { top_k, .. } => *top_k,
            _ => return Err(GraphError::NoIndexPlan),
        };
        Ok(SimilarityResponse {
            tenant_id: query.tenant_id,
            hits: Vec::new(),
            indices_used: plan.indices_used,
            query_hash: format!("mock-similarity-k{}", top_k),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_influence(
        &self,
        plan: PreparedPlan,
        query: &InfluenceQuery,
        degradation: Option<Degradation>,
    ) -> Result<InfluenceResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        let iterations = match &plan.plan {
            crate::plan::Plan::Influence { iterations, .. } => *iterations,
            _ => return Err(GraphError::NoIndexPlan),
        };
        if query.seeds.is_empty() {
            return Err(GraphError::InvalidQuery("seeds_required"));
        }
        let nodes: Vec<GraphNode> = query
            .seeds
            .iter()
            .map(|seed| GraphNode {
                id: GraphNodeRef::Event(*seed),
                kind: GraphNodeKind::Event,
                label: Some(format!("seed:{}", seed.as_u64())),
                summary: None,
                weight: Some(1.0 / query.seeds.len() as f32),
                metadata: Some(json!({ "iteration": iterations })),
            })
            .collect();
        let graph = GraphView {
            nodes,
            edges: Vec::new(),
            summary: Some("mock_influence".into()),
        };
        Ok(InfluenceResponse {
            tenant_id: query.tenant_id,
            graph,
            total_influence: 1.0,
            iterations,
            indices_used: plan.indices_used,
            query_hash: "mock-influence".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_pattern(
        &self,
        plan: PreparedPlan,
        query: &PatternQuery,
        degradation: Option<Degradation>,
    ) -> Result<PatternResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        let limit = match &plan.plan {
            crate::plan::Plan::Pattern { limit, .. } => *limit,
            _ => return Err(GraphError::NoIndexPlan),
        };
        let graph = GraphView {
            nodes: Vec::new(),
            edges: Vec::new(),
            summary: Some(format!("pattern:{}", query.template_id)),
        };
        Ok(PatternResponse {
            tenant_id: query.tenant_id,
            matches: vec![graph],
            indices_used: plan.indices_used,
            query_hash: format!("mock-pattern-{}", limit),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_recall(
        &self,
        plan: PreparedPlan,
        query: &RecallQuery,
        degradation: Option<Degradation>,
    ) -> Result<RecallResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        match &plan.plan {
            crate::plan::Plan::VectorAnn { filter, .. }
            | crate::plan::Plan::SparseBm25 { filter, .. } => {
                if let Some(expected_scenes) = &filter.scenes {
                    if let Some(actual_scenes) = &query.filters.scenes {
                        if !actual_scenes
                            .iter()
                            .all(|scene| expected_scenes.contains(scene))
                        {
                            return Err(GraphError::InvalidQuery("scene_mismatch"));
                        }
                    }
                }
                if let Some(expected_types) = &filter.event_types {
                    if let Some(actual_types) = &query.filters.event_types {
                        if !actual_types.iter().all(|ty| expected_types.contains(ty)) {
                            return Err(GraphError::InvalidQuery("event_type_mismatch"));
                        }
                    }
                }
            }
            crate::plan::Plan::Causal {
                scenario,
                event_types,
                ..
            } => {
                if let Some(expected) = scenario {
                    if let Some(actuals) = query.filters.scenes.as_ref() {
                        if !actuals.contains(expected) {
                            return Err(GraphError::InvalidQuery("scenario_mismatch"));
                        }
                    }
                }
                if let Some(expected) = event_types {
                    if let Some(actual) = query.filters.event_types.as_ref() {
                        if !actual.iter().all(|ty| expected.contains(ty)) {
                            return Err(GraphError::InvalidQuery("event_type_mismatch"));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(RecallResponse {
            tenant_id: query.tenant_id,
            hits: Vec::new(),
            indices_used: plan.indices_used,
            query_hash: "mock-recall".into(),
            degradation_reason: degradation.map(|d| d.reason),
            graph: None,
        })
    }

    pub fn execute_awareness(
        &self,
        plan: PreparedPlan,
        query: &AwarenessQuery,
    ) -> Result<AwarenessResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;
        if let crate::plan::Plan::Awareness { filters, .. } = &plan.plan {
            Self::ensure_option_match(
                &filters.awareness_cycle_id,
                &query.filters.awareness_cycle_id,
                "awareness_cycle_mismatch",
            )?;
            Self::ensure_option_match(
                &filters.parent_cycle_id,
                &query.filters.parent_cycle_id,
                "parent_cycle_mismatch",
            )?;
            Self::ensure_option_match(
                &filters.collab_scope_id,
                &query.filters.collab_scope_id,
                "collab_scope_mismatch",
            )?;
            Self::ensure_option_match(
                &filters.barrier_id,
                &query.filters.barrier_id,
                "barrier_mismatch",
            )?;
            Self::ensure_option_match(
                &filters.env_mode,
                &query.filters.env_mode,
                "env_mode_mismatch",
            )?;
            if let Some(expected_seq) = filters.inference_cycle_sequence {
                if query.filters.inference_cycle_sequence != Some(expected_seq) {
                    return Err(GraphError::InvalidQuery("ic_sequence_mismatch"));
                }
            }
            if let Some(expected_types) = &filters.event_types {
                if let Some(actual_types) = &query.filters.event_types {
                    if !actual_types.iter().all(|ty| expected_types.contains(ty)) {
                        return Err(GraphError::InvalidQuery("event_type_mismatch"));
                    }
                }
            }
            if let Some(expected_reasons) = &filters.degradation_reasons {
                if let Some(actual_reasons) = &query.filters.degradation_reasons {
                    if !actual_reasons
                        .iter()
                        .all(|reason| expected_reasons.contains(reason))
                    {
                        return Err(GraphError::InvalidQuery("degradation_mismatch"));
                    }
                }
            }
            if let Some(expected_kinds) = &filters.sync_point_kinds {
                if let Some(actual_kinds) = &query.filters.sync_point_kinds {
                    if !actual_kinds.iter().all(|k| expected_kinds.contains(k)) {
                        return Err(GraphError::InvalidQuery("sync_point_kind_mismatch"));
                    }
                }
            }
        }
        Ok(AwarenessResponse {
            tenant_id: query.tenant_id,
            events: Vec::new(),
            next: None,
            indices_used: plan.indices_used,
            query_hash: "mock-awareness".into(),
            degradation_reason: None,
        })
    }

    pub fn execute_explain_replay(
        &self,
        plan: PreparedPlan,
        query: &ExplainReplayQuery,
        degradation: Option<Degradation>,
    ) -> Result<ExplainReplayResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.as_u64())?;

        let mut filters = AwarenessFilters::default();
        filters.awareness_cycle_id = Some(query.awareness_cycle_id);
        filters.event_types = Some(
            query
                .event_types
                .clone()
                .unwrap_or_else(|| EXPLAIN_EVENT_TYPES_DEFAULT.to_vec()),
        );

        if filters
            .event_types
            .as_ref()
            .map(|events| events.contains(&AwarenessEventType::SyncPointReported))
            .unwrap_or(false)
        {
            filters.sync_point_kinds = Some(vec![
                SyncPointKind::ToolBarrier,
                SyncPointKind::ClarifyAnswered,
                SyncPointKind::HitlAbsorb,
            ]);
        }

        let awareness_query = AwarenessQuery {
            tenant_id: query.tenant_id,
            filters,
            limit: query.limit,
            after: query.after.clone(),
        };

        let awareness_response = self.execute_awareness(plan, &awareness_query)?;
        Ok(ExplainReplayResponse {
            tenant_id: awareness_response.tenant_id,
            segments: awareness_response
                .events
                .into_iter()
                .map(|event| ExplainReplaySegment { event })
                .collect(),
            next: awareness_response.next,
            indices_used: awareness_response.indices_used,
            query_hash: awareness_response.query_hash,
            degradation_reason: degradation
                .map(|d| d.reason)
                .or(awareness_response.degradation_reason),
        })
    }

    pub fn execute_live(
        &self,
        plan: PreparedPlan,
        sub: &LiveSubscribe,
    ) -> Result<Vec<LiveEventPointer>, GraphError> {
        self.ensure_tenant(sub.tenant_id.as_u64())?;
        if let crate::plan::Plan::Live {
            filters,
            rate,
            heartbeat_ms,
            idle_timeout_ms,
            max_buffer,
            backpressure_mode,
        } = &plan.plan
        {
            if (*rate as usize) > self.caps.live_max_per_tenant {
                return Err(GraphError::InvalidQuery("rate_exceeds_cap"));
            }
            if let Some(expected_scene) = filters.scene.as_ref() {
                if let Some(actual_scene) = sub.filters.scene.as_ref() {
                    if actual_scene != expected_scene {
                        return Err(GraphError::InvalidQuery("scene_mismatch"));
                    }
                }
            }
            if let Some(req_rate) = sub.max_rate {
                if *rate > req_rate && self.caps.live_max_per_tenant >= req_rate as usize {
                    return Err(GraphError::InvalidQuery("rate_clamped_without_consent"));
                }
            }
            if let Some(req_heartbeat) = sub.heartbeat_ms {
                if *heartbeat_ms < req_heartbeat {
                    return Err(GraphError::InvalidQuery("heartbeat_mismatch"));
                }
            }
            if let Some(req_idle) = sub.idle_timeout_ms {
                if *idle_timeout_ms < req_idle {
                    return Err(GraphError::InvalidQuery("idle_timeout_mismatch"));
                }
            }
            if let Some(req_buffer) = sub.max_buffer {
                if *max_buffer > req_buffer {
                    return Err(GraphError::InvalidQuery("buffer_clamped_without_consent"));
                }
            }
            if let Some(req_mode) = sub.backpressure_mode.as_ref() {
                if req_mode != backpressure_mode {
                    return Err(GraphError::InvalidQuery("backpressure_mismatch"));
                }
            }
        }
        Ok(Vec::new())
    }

    fn ensure_tenant(&self, tenant: u64) -> Result<(), GraphError> {
        if self.caps.allowed_tenants.contains(&tenant) {
            Ok(())
        } else {
            Err(GraphError::AuthForbidden)
        }
    }

    fn ensure_option_match<T: PartialEq + std::fmt::Debug>(
        expected: &Option<T>,
        actual: &Option<T>,
        err: &'static str,
    ) -> Result<(), GraphError> {
        if let Some(exp) = expected {
            if actual.as_ref() != Some(exp) {
                return Err(GraphError::InvalidQuery(err));
            }
        }
        Ok(())
    }
}

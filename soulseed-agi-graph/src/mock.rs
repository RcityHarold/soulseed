use std::collections::{HashMap, HashSet};

use crate::{
    api::{
        AwarenessFilters, AwarenessQuery, AwarenessResponse, CausalEdge, ExplainReplayQuery,
        ExplainReplayResponse, ExplainReplaySegment, RecallResponse, EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::{Degradation, GraphError},
    plan::PreparedPlan,
    scenario::scenario_rule,
    types::{ConceptNode, EmotionNode, SemanticEdge, SemanticEdgeKind, TopicNode},
    AwarenessEventType, DialogueEvent, EventId, LiveEventPointer, LiveSubscribe, RecallQuery,
    SyncPointKind, TimelineQuery, TimelineResponse,
};

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
        if !self.caps.allowed_tenants.contains(&event.tenant_id.0) {
            return Err(GraphError::AuthForbidden);
        }
        let key = (event.tenant_id.0, event.session_id.0, event.sequence_number);
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
        self.ensure_tenant(query.tenant_id.0)?;
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
        })
    }

    pub fn execute_causal(
        &self,
        plan: PreparedPlan,
        query: &crate::api::CausalQuery,
        degradation: Option<Degradation>,
    ) -> Result<crate::api::CausalResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.0)?;
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
        Ok(crate::api::CausalResponse {
            tenant_id: query.tenant_id,
            nodes: Vec::new(),
            edges: vec![CausalEdge {
                from: query.root_event,
                to: EventId(query.root_event.0 + 1),
                edge_type: "TRIGGERED".into(),
                weight: Some(0.7),
            }],
            concept_nodes: vec![ConceptNode {
                concept_id: format!("concept:{}", query.root_event.0),
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
            semantic_edges: vec![SemanticEdge {
                from: query.root_event.0.to_string(),
                to: format!("concept:{}", query.root_event.0),
                kind: SemanticEdgeKind::EventToConcept,
                weight: Some(0.7),
            }],
            indices_used: plan.indices_used,
            query_hash: "mock-causal".into(),
            degradation_reason: degradation.map(|d| d.reason),
        })
    }

    pub fn execute_recall(
        &self,
        plan: PreparedPlan,
        query: &RecallQuery,
        degradation: Option<Degradation>,
    ) -> Result<RecallResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.0)?;
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
        })
    }

    pub fn execute_awareness(
        &self,
        plan: PreparedPlan,
        query: &AwarenessQuery,
    ) -> Result<AwarenessResponse, GraphError> {
        self.ensure_tenant(query.tenant_id.0)?;
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
        self.ensure_tenant(query.tenant_id.0)?;

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
        self.ensure_tenant(sub.tenant_id.0)?;
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

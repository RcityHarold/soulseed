use std::collections::{HashMap, HashSet};

use crate::{
    api::{
        AwarenessFilters, AwarenessQuery, AwarenessResponse, CausalEdge, ExplainReplayQuery,
        ExplainReplayResponse, ExplainReplaySegment, RecallResponse, EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::{Degradation, GraphError},
    plan::PreparedPlan,
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
                        if !actual_scenes.iter().all(|scene| expected_scenes.contains(scene)) {
                            return Err(GraphError::InvalidQuery("scene_mismatch"));
                        }
                    }
                }
                if let Some(expected_types) = &filter.event_types {
                    if let Some(actual_types) = &query.filters.event_types {
                        if !actual_types
                            .iter()
                            .all(|ty| expected_types.contains(ty))
                        {
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
        if let crate::plan::Plan::Live { filters, rate } = &plan.plan {
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

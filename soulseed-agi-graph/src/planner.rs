use crate::{
    api::{
        AwarenessFilters, AwarenessQuery, CausalDir, CausalQuery, ExplainReplayQuery,
        LiveSubscribe, RecallQuery, RecallQueryTextOrVec, TimelineQuery,
        EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::{Degradation, GraphError},
    plan::{
        Plan, PlanComponent, PlannerConfig, PreparedPlan, IDX_AWARENESS_BARRIER,
        IDX_AWARENESS_COLLAB, IDX_AWARENESS_CYCLE, IDX_AWARENESS_ENV, IDX_AWARENESS_PARENT,
        IDX_EDGE, IDX_SESSION_ORDER, IDX_SPARSE, IDX_TIMELINE, IDX_VEC,
    },
    AwarenessEventType, SyncPointKind,
};

pub struct Planner {
    cfg: PlannerConfig,
}

impl Planner {
    pub fn new(cfg: PlannerConfig) -> Self {
        Self { cfg }
    }

    pub fn plan_timeline(
        &self,
        query: &TimelineQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let limit = query.limit.min(self.cfg.max_limit).max(1);
        let mut indices = Vec::new();
        let plan = if let Some(session) = query.session_id {
            indices.push(IDX_SESSION_ORDER.to_string());
            Plan::SessionOrder {
                index: IDX_SESSION_ORDER.to_string(),
                session_id: session,
                after: query.after.as_ref().map(|c| c.0.clone()),
                scenario: query.scenario.clone(),
            }
        } else {
            indices.push(IDX_TIMELINE.to_string());
            Plan::Timeline {
                index: IDX_TIMELINE.to_string(),
                window: query.window.clone(),
                order: ("timestamp_ms".to_string(), "event_id".to_string()),
                scenario: query.scenario.clone(),
                event_types: query.event_types.clone(),
            }
        };
        let hash = format!("timeline:tenant={}:limit={}", query.tenant_id.0, limit);
        Ok((
            PreparedPlan {
                plan,
                indices_used: indices.clone(),
            },
            hash,
            None,
        ))
    }

    pub fn plan_causal(
        &self,
        query: &CausalQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let requested = query.max_depth.unwrap_or(self.cfg.default_causal_depth);
        let depth = requested.min(self.cfg.max_causal_depth);
        let degradation = if requested > self.cfg.default_causal_depth {
            Some(Degradation {
                reason: "depth_truncated".into(),
            })
        } else {
            None
        };
        let dir = match query.direction {
            CausalDir::Upstream => "upstream".to_string(),
            CausalDir::Downstream => "downstream".to_string(),
            CausalDir::Both => "both".to_string(),
        };
        let plan = Plan::Causal {
            index: IDX_EDGE.to_string(),
            root: query.root_event,
            dir,
            depth,
            window: query.time_window.clone(),
            scenario: query.scenario.clone(),
            event_types: query.event_types.clone(),
        };
        let hash = format!(
            "causal:tenant={}:root={}:depth={}",
            query.tenant_id.0, query.root_event.0, depth
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_EDGE.into(), IDX_TIMELINE.into()],
            },
            hash,
            degradation,
        ))
    }

    pub fn plan_recall(
        &self,
        query: &RecallQuery,
        vector_available: bool,
        dim: u16,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let mut indices = Vec::new();
        let plan;
        let degradation;
        if vector_available {
            indices.push(IDX_VEC.into());
            plan = Plan::VectorAnn {
                index: IDX_VEC.to_string(),
                dim,
                k: query.k,
                filter: query.filters.clone(),
            };
            degradation = None;
        } else {
            indices.push(IDX_SPARSE.into());
            plan = Plan::SparseBm25 {
                index: IDX_SPARSE.to_string(),
                k: query.k,
                filter: query.filters.clone(),
            };
            degradation = Some(Degradation {
                reason: "sparse_only".into(),
            });
        }
        let hash = match &query.query {
            RecallQueryTextOrVec::Text(_) => {
                format!("recall:text:tenant={}:k={}", query.tenant_id.0, query.k)
            }
            RecallQueryTextOrVec::Vec(_) => {
                format!("recall:vec:tenant={}:k={}", query.tenant_id.0, query.k)
            }
        };
        Ok((
            PreparedPlan {
                plan,
                indices_used: indices,
            },
            hash,
            degradation,
        ))
    }

    pub fn plan_hybrid(&self, components: Vec<PlanComponent>, k: u16) -> PreparedPlan {
        PreparedPlan {
            plan: Plan::Hybrid { components, k },
            indices_used: vec![IDX_VEC.into(), IDX_SPARSE.into(), IDX_EDGE.into()],
        }
    }

    pub fn plan_live(
        &self,
        subscribe: &LiveSubscribe,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if subscribe.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let requested = subscribe
            .max_rate
            .unwrap_or(self.cfg.live_default_rate)
            .max(1);
        let rate = requested.min(self.cfg.live_max_rate);
        let plan = Plan::Live {
            filters: subscribe.filters.clone(),
            rate,
        };
        let hash = format!(
            "live:tenant={}:scene={:?}:participants={}",
            subscribe.tenant_id.0,
            subscribe.filters.scene,
            subscribe
                .filters
                .participants
                .as_ref()
                .map(|p| p.len())
                .unwrap_or(0)
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: Vec::new(),
            },
            hash,
            None,
        ))
    }

    pub fn plan_awareness(
        &self,
        query: &AwarenessQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }

        let limit = query.limit.min(self.cfg.max_limit).max(1);
        let mut indices = Vec::new();
        let mut primary = IDX_AWARENESS_CYCLE;

        if query.filters.barrier_id.is_some() {
            primary = IDX_AWARENESS_BARRIER;
        } else if query.filters.collab_scope_id.is_some() {
            primary = IDX_AWARENESS_COLLAB;
        } else if query.filters.parent_cycle_id.is_some() {
            primary = IDX_AWARENESS_PARENT;
        } else if query.filters.env_mode.is_some() {
            primary = IDX_AWARENESS_ENV;
        }

        indices.push(primary.to_string());
        if primary != IDX_AWARENESS_CYCLE {
            indices.push(IDX_AWARENESS_CYCLE.to_string());
        }

        let plan = Plan::Awareness {
            index: primary.to_string(),
            filters: query.filters.clone(),
            order: ("occurred_at_ms".into(), "event_id".into()),
            limit,
        };

        let hash = format!(
            "awareness:tenant={}:cycle={:?}:parent={:?}:scope={:?}:barrier={:?}:env={:?}:limit={}",
            query.tenant_id.0,
            query.filters.awareness_cycle_id,
            query.filters.parent_cycle_id,
            query.filters.collab_scope_id,
            query.filters.barrier_id,
            query.filters.env_mode,
            limit
        );

        Ok((
            PreparedPlan {
                plan,
                indices_used: indices,
            },
            hash,
            None,
        ))
    }

    pub fn plan_explain_replay(
        &self,
        query: &ExplainReplayQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.0 == 0 {
            return Err(GraphError::AuthForbidden);
        }

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

        let limit = query.limit.min(self.cfg.max_limit).max(1);
        let awareness_query = AwarenessQuery {
            tenant_id: query.tenant_id,
            filters,
            limit,
            after: query.after.clone(),
        };

        let (prepared, base_hash, _) = self.plan_awareness(&awareness_query)?;
        let degradation = query.forks.as_ref().map(|_| Degradation {
            reason: "fork_filter_not_supported".into(),
        });

        let hash = format!(
            "explain:{}:forks={}",
            base_hash,
            query
                .forks
                .as_ref()
                .map(|forks| forks.len())
                .unwrap_or(0)
        );

        Ok((prepared, hash, degradation))
    }
}

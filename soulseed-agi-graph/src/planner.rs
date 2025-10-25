use crate::{
    api::{
        AwarenessFilters, AwarenessQuery, CausalDir, CausalQuery, ExplainReplayQuery,
        InfluenceQuery, LiveSubscribe, NeighborhoodDirection, NeighborhoodQuery, PathQuery,
        PathStrategy, PatternQuery, RecallQuery, RecallQueryTextOrVec, SimilarityQuery,
        SimilarityStrategy, SubgraphQuery, TimelineQuery, EXPLAIN_EVENT_TYPES_DEFAULT,
    },
    errors::{Degradation, GraphError},
    plan::{
        Plan, PlanComponent, PlannerConfig, PreparedPlan, IDX_AWARENESS_BARRIER,
        IDX_AWARENESS_COLLAB, IDX_AWARENESS_CYCLE, IDX_AWARENESS_ENV, IDX_AWARENESS_PARENT,
        IDX_EDGE, IDX_GRAPH_INFLUENCE, IDX_GRAPH_NEIGHBORHOOD, IDX_GRAPH_PATH, IDX_GRAPH_PATTERN,
        IDX_GRAPH_SUBGRAPH, IDX_SESSION_ORDER, IDX_SPARSE, IDX_TIMELINE, IDX_VEC,
    },
    scenario::scenario_rule,
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
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let scenario_event_types = query
            .scenario
            .as_ref()
            .map(|scene| scenario_rule(scene).primary_event_types.to_vec());
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
                event_types: scenario_event_types
                    .clone()
                    .or_else(|| query.event_types.clone()),
            }
        };
        let hash = format!(
            "timeline:tenant={}:limit={}",
            query.tenant_id.as_u64(),
            limit
        );
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
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let scenario_event_types = query
            .scenario
            .as_ref()
            .map(|scene| scenario_rule(scene).primary_event_types.to_vec());
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
            event_types: scenario_event_types
                .clone()
                .or_else(|| query.event_types.clone()),
        };
        let hash = format!(
            "causal:tenant={}:root={}:depth={}",
            query.tenant_id.as_u64(),
            query.root_event.as_u64(),
            depth
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

    pub fn plan_path(
        &self,
        query: &PathQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        if query.start == query.goal {
            return Err(GraphError::InvalidQuery("path_requires_distinct_nodes"));
        }
        let requested_depth = query.max_depth.unwrap_or(self.cfg.default_causal_depth);
        let depth = requested_depth.min(self.cfg.max_path_depth);
        let mut degradations = Vec::new();
        if requested_depth > depth {
            degradations.push("depth_truncated");
        }
        let max_paths = query.max_paths.min(self.cfg.max_limit as u16);
        if query.max_paths > max_paths {
            degradations.push("path_count_truncated");
        }
        let strategy = match query.strategy {
            PathStrategy::Bfs => "bfs",
            PathStrategy::BidirectionalDijkstra => "bidirectional_dijkstra",
        }
        .to_string();
        let plan = Plan::Path {
            index: IDX_GRAPH_PATH.to_string(),
            start: query.start,
            goal: query.goal,
            strategy: strategy.clone(),
            max_depth: depth,
            max_paths,
            scenario: query.scenario.clone(),
        };
        let hash = format!(
            "path:tenant={}:start={}:goal={}:strategy={}",
            query.tenant_id.as_u64(),
            query.start.as_u64(),
            query.goal.as_u64(),
            strategy
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_GRAPH_PATH.into()],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_neighborhood(
        &self,
        query: &NeighborhoodQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let radius = query.radius.min(self.cfg.max_neighborhood_radius);
        let limit = query.limit.min(self.cfg.max_limit);
        let mut degradations = Vec::new();
        if query.radius > radius {
            degradations.push("radius_truncated");
        }
        if query.limit > limit {
            degradations.push("limit_truncated");
        }
        let direction = match query.direction {
            NeighborhoodDirection::Outbound => "outbound",
            NeighborhoodDirection::Inbound => "inbound",
            NeighborhoodDirection::Both => "both",
        }
        .to_string();
        let plan = Plan::Neighborhood {
            index: IDX_GRAPH_NEIGHBORHOOD.to_string(),
            center: query.center,
            direction,
            radius,
            limit,
            scenario: query.scenario.clone(),
        };
        let hash = format!(
            "neighborhood:tenant={}:center={}:radius={}",
            query.tenant_id.as_u64(),
            query.center.as_u64(),
            radius
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_GRAPH_NEIGHBORHOOD.into()],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_subgraph(
        &self,
        query: &SubgraphQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        if query.seeds.is_empty() {
            return Err(GraphError::InvalidQuery("seeds_required"));
        }
        let radius = query.radius.min(self.cfg.max_subgraph_radius);
        let max_nodes = query.max_nodes.min(self.cfg.max_subgraph_nodes);
        let mut degradations = Vec::new();
        if query.radius > radius {
            degradations.push("radius_truncated");
        }
        if query.max_nodes > max_nodes {
            degradations.push("node_limit_truncated");
        }
        let plan = Plan::Subgraph {
            index: IDX_GRAPH_SUBGRAPH.to_string(),
            seeds: query.seeds.clone(),
            radius,
            max_nodes,
            scenario: query.scenario.clone(),
            include_artifacts: query.include_artifacts.unwrap_or(false),
        };
        let hash = format!(
            "subgraph:tenant={}:seeds={}:radius={}",
            query.tenant_id.as_u64(),
            query.seeds.len(),
            radius
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_GRAPH_SUBGRAPH.into()],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_recall(
        &self,
        query: &RecallQuery,
        vector_available: bool,
        dim: u16,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
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
                format!(
                    "recall:text:tenant={}:k={}",
                    query.tenant_id.as_u64(),
                    query.k
                )
            }
            RecallQueryTextOrVec::Vec(_) => {
                format!(
                    "recall:vec:tenant={}:k={}",
                    query.tenant_id.as_u64(),
                    query.k
                )
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

    pub fn plan_similarity(
        &self,
        query: &SimilarityQuery,
        vector_available: bool,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let top_k = query.top_k.min(self.cfg.max_similarity_k);
        let mut degradations = Vec::new();
        if query.top_k > top_k {
            degradations.push("similarity_k_truncated");
        }
        let mut index = IDX_VEC.to_string();
        let mut strategy = query.strategy.clone();
        if !vector_available {
            if matches!(strategy, SimilarityStrategy::Vector) {
                degradations.push("vector_index_unavailable");
                strategy = SimilarityStrategy::Hybrid;
            }
            index = IDX_SPARSE.to_string();
        }
        let plan = Plan::Similarity {
            index: index.clone(),
            anchor: query.anchor_event,
            top_k,
            strategy: match strategy {
                SimilarityStrategy::Vector => "vector",
                SimilarityStrategy::Hybrid => "hybrid",
            }
            .to_string(),
            filter: query.filters.clone(),
        };
        let hash = format!(
            "similarity:tenant={}:anchor={}:k={}",
            query.tenant_id.as_u64(),
            query.anchor_event.as_u64(),
            top_k
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![index],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_hybrid(&self, components: Vec<PlanComponent>, k: u16) -> PreparedPlan {
        PreparedPlan {
            plan: Plan::Hybrid { components, k },
            indices_used: vec![IDX_VEC.into(), IDX_SPARSE.into(), IDX_EDGE.into()],
        }
    }

    pub fn plan_influence(
        &self,
        query: &InfluenceQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        if query.seeds.is_empty() {
            return Err(GraphError::InvalidQuery("seeds_required"));
        }
        let iterations = query.iterations.min(self.cfg.max_influence_iterations);
        let mut degradations = Vec::new();
        if query.iterations > iterations {
            degradations.push("iterations_truncated");
        }
        let damping = query.damping_factor.clamp(0.0, 1.0);
        if (query.damping_factor - damping).abs() > f32::EPSILON {
            degradations.push("damping_clamped");
        }
        let plan = Plan::Influence {
            index: IDX_GRAPH_INFLUENCE.to_string(),
            seeds: query.seeds.clone(),
            horizon_ms: query.horizon_ms,
            damping_factor: damping,
            iterations,
            scenario: query.scenario.clone(),
        };
        let hash = format!(
            "influence:tenant={}:seeds={}:iterations={}",
            query.tenant_id.as_u64(),
            query.seeds.len(),
            iterations
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_GRAPH_INFLUENCE.into()],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_pattern(
        &self,
        query: &PatternQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let limit = query.limit.min(self.cfg.max_pattern_limit);
        let mut degradations = Vec::new();
        if query.limit > limit {
            degradations.push("limit_truncated");
        }
        let plan = Plan::Pattern {
            index: IDX_GRAPH_PATTERN.to_string(),
            template_id: query.template_id.clone(),
            limit,
        };
        let hash = format!(
            "pattern:tenant={}:template={}:limit={}",
            query.tenant_id.as_u64(),
            query.template_id,
            limit
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: vec![IDX_GRAPH_PATTERN.into()],
            },
            hash,
            join_degradations(degradations),
        ))
    }

    pub fn plan_live(
        &self,
        subscribe: &LiveSubscribe,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if subscribe.tenant_id.as_u64() == 0 {
            return Err(GraphError::AuthForbidden);
        }
        let requested_rate = subscribe
            .max_rate
            .unwrap_or(self.cfg.live_default_rate)
            .max(1);
        let rate = requested_rate.min(self.cfg.live_max_rate);
        let heartbeat_requested = subscribe
            .heartbeat_ms
            .unwrap_or(self.cfg.live_default_heartbeat_ms);
        let heartbeat_ms = heartbeat_requested.max(self.cfg.live_min_heartbeat_ms);
        let idle_requested = subscribe
            .idle_timeout_ms
            .unwrap_or(self.cfg.live_default_idle_timeout_ms);
        let idle_timeout_ms = idle_requested.max(heartbeat_ms * 2);
        let buffer_requested = subscribe.max_buffer.unwrap_or(self.cfg.live_max_buffer);
        let max_buffer = buffer_requested.min(self.cfg.live_max_buffer).max(1);
        let backpressure_mode = subscribe
            .backpressure_mode
            .clone()
            .unwrap_or_else(|| self.cfg.live_default_backpressure_mode.clone());
        let mut degradation = None;
        if rate != requested_rate {
            degradation = Some(Degradation {
                reason: "rate_clamped".into(),
            });
        }
        if max_buffer != buffer_requested && degradation.is_none() {
            degradation = Some(Degradation {
                reason: "buffer_clamped".into(),
            });
        }
        let plan = Plan::Live {
            filters: subscribe.filters.clone(),
            rate,
            heartbeat_ms,
            idle_timeout_ms,
            max_buffer,
            backpressure_mode: backpressure_mode.clone(),
        };
        let hash = format!(
            "live:tenant={}:scene={:?}:participants={}:rate={}:heartbeat={}:buffer={}:bp={}",
            subscribe.tenant_id.as_u64(),
            subscribe.filters.scene,
            subscribe
                .filters
                .participants
                .as_ref()
                .map(|p| p.len())
                .unwrap_or(0),
            rate,
            heartbeat_ms,
            max_buffer,
            backpressure_mode
        );
        Ok((
            PreparedPlan {
                plan,
                indices_used: Vec::new(),
            },
            hash,
            degradation,
        ))
    }

    pub fn plan_awareness(
        &self,
        query: &AwarenessQuery,
    ) -> Result<(PreparedPlan, String, Option<Degradation>), GraphError> {
        if query.tenant_id.as_u64() == 0 {
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
            query.tenant_id.as_u64(),
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
        if query.tenant_id.as_u64() == 0 {
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
            query.forks.as_ref().map(|forks| forks.len()).unwrap_or(0)
        );

        Ok((prepared, hash, degradation))
    }
}

fn join_degradations(degradations: Vec<&'static str>) -> Option<Degradation> {
    if degradations.is_empty() {
        None
    } else {
        Some(Degradation {
            reason: degradations.join("+"),
        })
    }
}

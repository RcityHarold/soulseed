use std::collections::HashMap;

use serde_json::{json, Value};
use xxhash_rust::xxh3::xxh3_64;

use crate::config::ToolsConfig;
use crate::dto::{
    CandidateScore, ExplainPlan, OrchestrationMode, PlanLineage, PlannerContext, ToolBarrier,
    ToolCallSpec, ToolEdge, ToolEdgeKind, ToolGraph, ToolLane, ToolPlan,
};
use crate::errors::PlannerError;
use crate::traits::Planner;

pub struct ToolPlanner;

impl ToolPlanner {
    fn build_spec(
        candidate: &CandidateScore,
        catalog: &HashMap<String, crate::dto::RouterCandidate>,
        request_context: &Value,
        anchor: &crate::dto::Anchor,
        index: usize,
    ) -> ToolCallSpec {
        let metadata = catalog.get(&candidate.tool_id);
        let default_input = serde_json::json!({ "tool_id": candidate.tool_id });
        let default_params = serde_json::json!({});
        let inputs = request_context
            .get("inputs")
            .and_then(Value::as_object)
            .and_then(|map| map.get(&candidate.tool_id))
            .cloned()
            .unwrap_or(default_input);
        let params = request_context
            .get("params")
            .and_then(Value::as_object)
            .and_then(|map| map.get(&candidate.tool_id))
            .cloned()
            .unwrap_or(default_params);

        let cacheable = metadata.map(|meta| !meta.side_effect).unwrap_or_else(|| {
            request_context
                .get("cacheable")
                .and_then(Value::as_object)
                .and_then(|map| map.get(&candidate.tool_id))
                .and_then(Value::as_bool)
                .unwrap_or(true)
        });
        let stream = metadata
            .map(|meta| meta.supports_stream)
            .unwrap_or_else(|| {
                request_context
                    .get("stream")
                    .and_then(Value::as_object)
                    .and_then(|map| map.get(&candidate.tool_id))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            });

        ToolCallSpec {
            tool_id: candidate.tool_id.clone(),
            schema_v: metadata
                .map(|m| m.version.parse::<u16>().unwrap_or(1))
                .unwrap_or(1),
            input: inputs,
            params,
            cacheable,
            stream,
            idem_key: format!("{}:{}", anchor.envelope_id, index),
            supersedes: None,
            superseded_by: None,
        }
    }

    fn plan_hash(anchor: &crate::dto::Anchor, specs: &[ToolCallSpec], cfg: &ToolsConfig) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&anchor.tenant_id.into_inner().to_le_bytes());
        bytes.extend_from_slice(&anchor.envelope_id.as_u128().to_le_bytes());
        bytes.extend_from_slice(cfg.snapshot_hash.as_bytes());
        bytes.extend_from_slice(&cfg.snapshot_version.to_le_bytes());
        for spec in specs {
            bytes.extend_from_slice(spec.tool_id.as_bytes());
            bytes.extend_from_slice(&spec.schema_v.to_le_bytes());
            bytes.extend_from_slice(spec.idem_key.as_bytes());
        }
        let hash = xxh3_64(&bytes);
        format!("plan-{:016x}", hash)
    }

    fn build_explain(
        ranked: &[CandidateScore],
        excluded: &[(String, String)],
        policy_digest: &Option<String>,
        cfg: &ToolsConfig,
        schema_v: u16,
    ) -> ExplainPlan {
        let weights = &cfg.router.weights;
        let weights_json = json!({
            "context_fit": weights.context_fit,
            "success_rate": weights.success_rate,
            "latency_p95": weights.latency_p95,
            "cost_per_success": weights.cost_per_success,
            "risk_level": weights.risk_level,
            "auth_impact": weights.auth_impact,
            "cacheability": weights.cacheability,
            "degradation_acceptance": weights.degradation_acceptance,
        });
        ExplainPlan {
            schema_v,
            candidates: ranked.to_vec(),
            excluded: excluded.to_vec(),
            weights: weights_json,
            policy_digest: policy_digest.clone(),
        }
    }

    fn graph_from_request_context(ctx: &Value, node_count: usize) -> Option<ToolGraph> {
        let graph_obj = ctx.get("graph")?.as_object()?;

        let mut edges = Vec::new();
        if let Some(edge_vals) = graph_obj.get("edges").and_then(Value::as_array) {
            for edge in edge_vals {
                let edge_obj = edge.as_object()?;
                let from = edge_obj.get("from").and_then(Value::as_u64)? as usize;
                let to = edge_obj.get("to").and_then(Value::as_u64)? as usize;
                if from >= node_count || to >= node_count || from == to {
                    continue;
                }
                let kind = edge_obj
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("sequential")
                    .to_ascii_lowercase();
                let kind = match kind.as_str() {
                    "dependency" => ToolEdgeKind::Dependency,
                    "barrier" => ToolEdgeKind::Barrier,
                    _ => ToolEdgeKind::Sequential,
                };
                edges.push(ToolEdge { from, to, kind });
            }
        }

        let mut barriers = Vec::new();
        if let Some(barrier_vals) = graph_obj.get("barriers").and_then(Value::as_array) {
            for barrier in barrier_vals {
                let barrier_obj = barrier.as_object()?;
                let nodes = barrier_obj
                    .get("nodes")
                    .and_then(Value::as_array)
                    .map(|vals| {
                        vals.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as usize))
                            .filter(|n| *n < node_count)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if nodes.is_empty() {
                    continue;
                }
                let barrier_id = barrier_obj
                    .get("barrier_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                barriers.push(ToolBarrier { barrier_id, nodes });
            }
        }

        Some(ToolGraph { edges, barriers })
    }
}

impl Planner for ToolPlanner {
    fn build_plan(&self, ctx: PlannerContext, cfg: &ToolsConfig) -> Result<ToolPlan, PlannerError> {
        if ctx.anchor.config_snapshot_hash != cfg.snapshot_hash
            || ctx.anchor.config_snapshot_version != cfg.snapshot_version
        {
            return Err(PlannerError::ConfigInvalid("snapshot_mismatch".into()));
        }
        if ctx.ranked.is_empty() {
            return Err(PlannerError::NoCandidates);
        }

        let PlannerContext {
            anchor,
            ranked,
            policy_digest,
            excluded,
            scene,
            capability_hints: _,
            request_context,
            catalog,
            subject,
        } = ctx;

        let catalog: HashMap<String, crate::dto::RouterCandidate> = catalog
            .into_iter()
            .map(|c| (c.tool_id.clone(), c))
            .collect();

        let specs: Vec<ToolCallSpec> = ranked
            .iter()
            .enumerate()
            .map(|(idx, candidate)| {
                Self::build_spec(candidate, &catalog, &request_context, &anchor, idx)
            })
            .collect();

        let plan_id = Self::plan_hash(&anchor, &specs, cfg);
        let explain_plan =
            Self::build_explain(&ranked, &excluded, &policy_digest, cfg, anchor.schema_v);
        let lane = anchor.lane(&scene);

        let mut strategy = cfg.to_strategy();
        match lane {
            ToolLane::SelfReflection => {
                strategy.mode = OrchestrationMode::Serial;
                strategy.retry.max_retries = strategy.retry.max_retries.min(1);
                strategy.timeout_ms = strategy
                    .timeout_ms
                    .saturating_sub(strategy.timeout_ms / 3)
                    .max(120);
                if let Some(deadline) = strategy.deadline_ms.as_mut() {
                    *deadline = (*deadline).min(1000);
                }
            }
            ToolLane::Collaboration => {
                strategy.mode = OrchestrationMode::Hybrid;
                strategy.retry.max_retries = strategy.retry.max_retries.max(2);
                strategy.timeout_ms = (strategy.timeout_ms as f32 * 1.2).round() as u32;
            }
            ToolLane::ToolInvocation => {}
        }

        let mut budget = cfg.to_budget();
        match lane {
            ToolLane::SelfReflection => {
                budget.max_cost_tokens = budget.max_cost_tokens.min(4_000);
                budget.max_latency_ms = budget.max_latency_ms.min(900);
            }
            ToolLane::Collaboration => {
                budget.max_cost_tokens = budget.max_cost_tokens.saturating_add(2_000);
                budget.max_latency_ms = (budget.max_latency_ms as f32 * 1.5).round() as u32;
            }
            ToolLane::ToolInvocation => {}
        }

        let mut graph = Self::graph_from_request_context(&request_context, specs.len())
            .unwrap_or_else(|| {
                let mut built = match strategy.mode {
                    OrchestrationMode::Parallel => ToolGraph::parallel(specs.len()),
                    OrchestrationMode::Serial => ToolGraph::linear(specs.len()),
                    OrchestrationMode::Hybrid => ToolGraph::with_barrier(specs.len()),
                };

                if matches!(lane, ToolLane::Collaboration) && specs.len() > 1 {
                    built = ToolGraph::with_barrier(specs.len());
                }
                built
            });
        graph.assign_barrier_ids(&plan_id);

        let plan_schema_v = anchor.schema_v;
        let lineage = PlanLineage {
            version: anchor
                .sequence_number
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
            supersedes: anchor.supersedes.as_ref().map(|id| id.to_string()),
            superseded_by: anchor.superseded_by.as_ref().map(|id| id.to_string()),
        };

        Ok(ToolPlan {
            plan_id,
            anchor,
            schema_v: plan_schema_v,
            lineage,
            subject,
            items: specs,
            strategy,
            budget,
            explain_plan,
            lane,
            graph,
        })
    }
}

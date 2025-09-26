use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use serde_json::json;
use time::OffsetDateTime;

use crate::dto::{ExplainRun, OrchestratorResult, RunStage, ToolPlan, ToolResultSummary};
use crate::errors::{OrchestratorError, ToolError};
use crate::metrics::Observability;
use crate::traits::Orchestrator;
use crate::tw_client::{PrechargeDecision, Subject, ThinWaistClient, TwEvent};

pub struct ToolOrchestrator;

impl ToolOrchestrator {
    fn resolve_subject(plan_subject: &Option<crate::dto::Subject>) -> Subject {
        match plan_subject {
            Some(crate::dto::Subject::Human(id)) => Subject {
                human_id: Some(*id),
                ai_id: None,
            },
            Some(crate::dto::Subject::AI(id)) => Subject {
                human_id: None,
                ai_id: Some(*id),
            },
            _ => Subject {
                human_id: None,
                ai_id: None,
            },
        }
    }

    fn now_ms() -> u64 {
        OffsetDateTime::now_utc().unix_timestamp_nanos() as u64 / 1_000_000
    }

    fn should_fallback(rules: &[crate::dto::FallbackRule], reason: &str) -> bool {
        let reason_upper = reason.to_ascii_uppercase();
        rules.iter().any(|rule| {
            let on_upper = rule.on.to_ascii_uppercase();
            reason_upper.contains(&on_upper)
        })
    }

    fn record_stage(
        stages: &mut Vec<RunStage>,
        tool_id: &str,
        outcome: &str,
        note: Option<String>,
    ) {
        let start = SystemTime::now();
        let start_ms = Self::now_ms();
        // simulate short duration to keep deterministic scale
        let end_ms = start
            .checked_add(Duration::from_millis(1))
            .map(|_| start_ms + 1)
            .unwrap_or(start_ms);
        stages.push(RunStage {
            tool_id: tool_id.into(),
            action: "execute".into(),
            start_ms,
            end_ms,
            outcome: outcome.into(),
            notes: note,
        });
    }
}

impl Orchestrator for ToolOrchestrator {
    fn execute_plan(
        &self,
        plan: &ToolPlan,
        client: &dyn ThinWaistClient,
        obs: &dyn Observability,
    ) -> Result<OrchestratorResult, OrchestratorError> {
        let lane = plan.lane;
        let lane_tag = lane.metric_tag().to_string();
        let subject = Self::resolve_subject(&plan.subject);
        let mut stages = Vec::new();
        let mut degradation_reason = None;
        let mut indices_used: Vec<String> = Vec::new();
        let mut query_hash = None;
        let mut fallback_triggered = false;
        let mut collected_summaries: Vec<ToolResultSummary> = Vec::new();
        let mut per_tool_degradation: HashMap<String, Option<String>> = HashMap::new();

        let run_parallel = matches!(plan.strategy.mode, crate::dto::OrchestrationMode::Parallel);
        let allow_hybrid_stream =
            matches!(plan.strategy.mode, crate::dto::OrchestrationMode::Hybrid);
        let mut has_success = false;

        let order = plan.graph.topo_order(plan.items.len());
        let mut executed = vec![false; plan.items.len()];
        let mut completed_barriers = HashSet::new();

        for idx in order {
            if idx >= plan.items.len() {
                continue;
            }
            let spec = &plan.items[idx];

            if has_success && !run_parallel {
                let allow_post_success = allow_hybrid_stream && spec.stream;
                if !allow_post_success {
                    break;
                }
            }

            let precharge = client.precharge(&plan.anchor, &subject, &spec.tool_id, &spec.idem_key);
            match precharge {
                Ok(PrechargeDecision { decision, .. }) if decision != "Allow" => {
                    Self::record_stage(
                        &mut stages,
                        &spec.tool_id,
                        "fallback",
                        Some(decision.clone()),
                    );
                    fallback_triggered = true;
                    per_tool_degradation.insert(spec.tool_id.clone(), Some(decision));
                    executed[idx] = true;
                    if has_success && (run_parallel || allow_hybrid_stream) {
                        continue;
                    }
                    continue;
                }
                Err(err) => {
                    let reason = err.to_string();
                    let normalized = reason.to_ascii_uppercase();
                    Self::record_stage(&mut stages, &spec.tool_id, "error", Some(reason.clone()));
                    if Self::should_fallback(&plan.strategy.fallback, &normalized)
                        || has_success && (run_parallel || allow_hybrid_stream)
                    {
                        fallback_triggered = true;
                        executed[idx] = true;
                        continue;
                    }
                    return Err(OrchestratorError::ExecutionFailed(reason));
                }
                _ => {}
            }

            let execution = client.execute(&plan.anchor, &subject, spec);
            match execution {
                Ok(res) => {
                    let tool_degrade = res.degradation_reason.clone();
                    client
                        .reconcile(&plan.anchor, &spec.idem_key, &json!({"status": "ok"}))
                        .map_err(|e| OrchestratorError::ExecutionFailed(e.to_string()))?;
                    degradation_reason = degradation_reason.or(tool_degrade.clone());
                    if let Some(indices) = res.indices_used.clone() {
                        for idx_used in indices {
                            if !indices_used.contains(&idx_used) {
                                indices_used.push(idx_used);
                            }
                        }
                    }
                    query_hash = query_hash.or(res.query_hash.clone());
                    let mut note = None;
                    if has_success && !run_parallel {
                        note = Some("post_success".into());
                    }
                    Self::record_stage(&mut stages, &spec.tool_id, "ok", note);
                    collected_summaries.push(res.summary.clone());
                    per_tool_degradation.insert(spec.tool_id.clone(), tool_degrade);
                    has_success = true;

                    obs.emit_metric(
                        "tool_orchestrate_latency_ms",
                        plan.strategy.timeout_ms as f64,
                        &[
                            ("mode", format!("{:?}", plan.strategy.mode)),
                            ("tool_id", spec.tool_id.clone()),
                            ("lane", lane_tag.clone()),
                        ],
                    );
                    client
                        .emit_events(&[TwEvent::ToolResponded {
                            tool_id: spec.tool_id.clone(),
                            idem_key: spec.idem_key.clone(),
                        }])
                        .map_err(|err| OrchestratorError::ExecutionFailed(err.to_string()))?;
                }
                Err(err) => {
                    let reason = err.to_string();
                    let normalized = reason.to_ascii_uppercase();
                    Self::record_stage(&mut stages, &spec.tool_id, "error", Some(reason.clone()));
                    client
                        .emit_events(&[TwEvent::ToolFailed {
                            tool_id: spec.tool_id.clone(),
                            idem_key: spec.idem_key.clone(),
                            reason: reason.clone(),
                        }])
                        .map_err(|err| OrchestratorError::ExecutionFailed(err.to_string()))?;
                    per_tool_degradation.insert(spec.tool_id.clone(), Some(reason.clone()));
                    if Self::should_fallback(&plan.strategy.fallback, &normalized)
                        || has_success && (run_parallel || allow_hybrid_stream)
                    {
                        fallback_triggered = true;
                        executed[idx] = true;
                        continue;
                    }
                    return Err(OrchestratorError::ExecutionFailed(reason));
                }
            }

            executed[idx] = true;

            for barrier in plan.graph.barriers_containing(idx) {
                if completed_barriers.contains(&barrier.barrier_id) {
                    continue;
                }
                if barrier
                    .nodes
                    .iter()
                    .all(|node| executed.get(*node).copied().unwrap_or(false))
                {
                    stages.push(RunStage {
                        tool_id: barrier.barrier_id.clone(),
                        action: "barrier_sync".into(),
                        start_ms: Self::now_ms(),
                        end_ms: Self::now_ms(),
                        outcome: "synced".into(),
                        notes: Some(format!("nodes={:?}", barrier.nodes)),
                    });
                    completed_barriers.insert(barrier.barrier_id.clone());
                }
            }
        }

        if fallback_triggered && degradation_reason.is_none() {
            degradation_reason = Some(lane.fallback_reason().into());
        }

        let meta = crate::dto::PlanExecutionMeta {
            degradation_reason: degradation_reason.clone(),
            indices_used: if indices_used.is_empty() {
                None
            } else {
                Some(indices_used)
            },
            query_hash,
            per_tool_degradation: per_tool_degradation
                .into_iter()
                .collect::<Vec<(String, Option<String>)>>(),
        };

        Ok(OrchestratorResult {
            summary: collected_summaries.last().cloned(),
            explain_run: ExplainRun {
                stages,
                degradation_reason,
                indices_used: meta.indices_used.clone(),
                query_hash: meta.query_hash.clone(),
                collected_summaries,
            },
            fallback_triggered,
            meta,
        })
    }
}

impl From<ToolError> for OrchestratorError {
    fn from(value: ToolError) -> Self {
        OrchestratorError::ExecutionFailed(value.to_string())
    }
}

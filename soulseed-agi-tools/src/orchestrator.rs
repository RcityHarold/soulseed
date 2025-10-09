use crate::dto::{ExplainRun, OrchestratorResult, PlanExecutionMeta, RunStage, ToolPlan};
use crate::errors::OrchestratorError;
use crate::metrics::Observability;
use crate::traits::Orchestrator;
use time::OffsetDateTime;

pub struct ToolOrchestrator;

impl ToolOrchestrator {
    fn now_ms() -> u64 {
        (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as u64
    }
}

impl Orchestrator for ToolOrchestrator {
    fn execute_plan(
        &self,
        plan: &ToolPlan,
        obs: &dyn Observability,
    ) -> Result<OrchestratorResult, OrchestratorError> {
        let mut stages = Vec::new();
        for spec in &plan.items {
            let ts = Self::now_ms();
            stages.push(RunStage {
                tool_id: spec.tool_id.clone(),
                action: "schedule".into(),
                start_ms: ts,
                end_ms: ts,
                outcome: "pending_execution".into(),
                notes: Some("delegated_to_soulbase".into()),
            });
        }

        let mut per_tool_degradation: Vec<(String, Option<String>)> = plan
            .budget
            .degrade_rules
            .iter()
            .map(|rule| (rule.from.clone(), Some(rule.reason.clone())))
            .collect();
        per_tool_degradation.sort_by(|a, b| a.0.cmp(&b.0));
        per_tool_degradation.dedup();

        let explain_run = ExplainRun {
            schema_v: plan.schema_v,
            stages,
            degradation_reason: None,
            indices_used: None,
            query_hash: None,
            collected_summaries: Vec::new(),
        };

        let meta = PlanExecutionMeta {
            degradation_reason: None,
            indices_used: None,
            query_hash: None,
            per_tool_degradation,
        };

        obs.emit_metric(
            "tool_plan_delegated_total",
            1.0,
            &[
                ("lane", plan.lane.metric_tag().to_string()),
                ("plan_id", plan.plan_id.clone()),
            ],
        );

        Ok(OrchestratorResult {
            summary: None,
            explain_run,
            fallback_triggered: false,
            meta,
        })
    }
}

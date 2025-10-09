use crate::dto::{ExplainRun, OrchestratorResult, ToolPlan};
use crate::traits::Explainer;

pub struct DefaultExplainer;

impl Explainer for DefaultExplainer {
    fn decorate(&self, plan: &ToolPlan, result: &mut OrchestratorResult) {
        if result.explain_run.degradation_reason.is_none() && result.fallback_triggered {
            result.explain_run.degradation_reason = Some("fallback_triggered".into());
        }
        if result.explain_run.stages.is_empty() {
            result.explain_run = ExplainRun {
                schema_v: plan.schema_v,
                stages: Vec::new(),
                degradation_reason: None,
                indices_used: None,
                query_hash: None,
                collected_summaries: Vec::new(),
            };
        }
    }
}

use crate::dto::{
    ExecutionContext, LlmExplain, LlmPlanExplain, LlmRunExplain, ModelCandidate, PlannedLlm,
    build_final_event,
};
use serde_json::{Value, json};
use soulseed_agi_core_models::DialogueEvent;

#[derive(Clone, Debug)]
pub struct ExplainPackage {
    pub explain: LlmExplain,
    pub final_event: DialogueEvent,
}

#[derive(Clone, Default)]
pub struct LlmExplainer;

impl LlmExplainer {
    pub fn build(&self, planned: &PlannedLlm, context: &ExecutionContext) -> ExplainPackage {
        let model = &context.intent.model;
        let result = &context.result;
        let degradation_chain = context.degradation_chain.clone();
        let indices_used = context.indices_used.clone();
        let query_hash = context.query_hash.clone();
        let decision = &planned.model_decision;

        let mut candidates = Vec::with_capacity(decision.candidates.len() + 1);
        candidates.push(ModelCandidate::selected(model.clone()));
        for candidate in &decision.candidates {
            if candidate.profile.model_id != model.model_id {
                candidates.push(candidate.clone());
            }
        }

        let policy_trace: Value = if decision.policy_trace.is_null() {
            json!({
                "source": "llm_router",
                "scene": planned.input.scene,
                "planner_hint": planned.plan.model_hint,
            })
        } else {
            decision.policy_trace.clone()
        };

        let run_provider = if result.provider_metadata.is_null() {
            json!({
                "source": "llm_thin_waist",
                "reasoning_visibility": result.reasoning_visibility,
                "redacted": result.redacted,
            })
        } else {
            result.provider_metadata.clone()
        };

        let explain = LlmExplain {
            schema_v: planned.plan.schema_v,
            lineage: planned.plan.lineage.clone(),
            anchor: planned.plan.anchor.clone(),
            plan: LlmPlanExplain {
                selected: model.clone(),
                candidates,
                policy_trace,
            },
            run: LlmRunExplain {
                degradation_reason: degradation_chain.clone(),
                indices_used: indices_used.clone(),
                query_hash: query_hash.clone(),
                usage: result.usage.clone(),
                provider_metadata: run_provider,
            },
        };

        let final_event = build_final_event(
            &context.intent.anchor,
            &planned.input.scene,
            result,
            model,
            degradation_chain.clone(),
            indices_used.clone(),
            query_hash.clone(),
        );

        ExplainPackage {
            explain,
            final_event,
        }
    }
}

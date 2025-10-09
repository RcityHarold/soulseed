use crate::dto::{LlmExplain, LlmInput, ModelProfile, build_final_event};
use crate::orchestrator::ExecutionOutcome;
use soulseed_agi_core_models::DialogueEvent;
use serde_json::json;

#[derive(Clone, Debug)]
pub struct ExplainPackage {
    pub explain: LlmExplain,
    pub final_event: DialogueEvent,
}

#[derive(Clone, Default)]
pub struct LlmExplainer;

impl LlmExplainer {
    pub fn build(
        &self,
        input: &LlmInput,
        model: &ModelProfile,
        outcome: &ExecutionOutcome,
    ) -> ExplainPackage {
        let explain = LlmExplain {
            model_id: model.model_id.clone(),
            policy_digest: model.policy_digest.clone(),
            degradation_reason: outcome.degradation_chain.clone(),
            indices_used: outcome.indices_used.clone(),
            query_hash: outcome.query_hash.clone(),
            usage_rank: model.selection_rank,
            usage_score: model.selection_score,
            estimated_cost_usd: model
                .estimated_cost_usd
                .or(outcome.result.usage.total_cost_usd),
            usage_band: model.usage_band.clone(),
            should_use_factors: if !model.should_use_factors.is_null() {
                model.should_use_factors.clone()
            } else {
                json!({
                    "source": "llm_engine",
                    "reasoning_visibility": outcome.result.reasoning_visibility,
                    "usage_tokens": {
                        "prompt": outcome.result.usage.prompt_tokens,
                        "completion": outcome.result.usage.completion_tokens,
                    },
                    "degradation_chain": outcome.degradation_chain
                })
            },
        };

        let final_event = build_final_event(
            &input.anchor,
            &input.scene,
            &outcome.result,
            model,
            outcome.degradation_chain.clone(),
            outcome.indices_used.clone(),
            outcome.query_hash.clone(),
        );

        ExplainPackage {
            explain,
            final_event,
        }
    }
}

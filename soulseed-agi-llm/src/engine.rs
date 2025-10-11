use crate::dto::{
    ExecutionContext, LlmInput, LlmOutput, LlmResult, ModelProfile, PlannedLlm, PromptBundle,
};
use crate::errors::EngineError;
use crate::explainer::{ExplainPackage, LlmExplainer};
use crate::orchestrator::LlmOrchestrator;
use crate::planner::LlmPlanner;
use crate::router::LlmRouter;
use crate::tw_client::ThinWaistClient;

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub system_prompt: String,
    pub planner_hint: Option<String>,
    pub safety_tier: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are Soulseed AGI LLM. Provide a concise final answer.".into(),
            planner_hint: None,
            safety_tier: "standard".into(),
        }
    }
}

pub struct LlmEngine<'a, C: ThinWaistClient> {
    router: LlmRouter,
    planner: LlmPlanner,
    orchestrator: LlmOrchestrator,
    explainer: LlmExplainer,
    client: &'a C,
    config: LlmConfig,
}

impl<'a, C: ThinWaistClient> LlmEngine<'a, C> {
    pub fn new(client: &'a C, config: LlmConfig) -> Self {
        Self {
            router: LlmRouter::default(),
            planner: LlmPlanner::default(),
            orchestrator: LlmOrchestrator::default(),
            explainer: LlmExplainer::default(),
            client,
            config,
        }
    }

    pub fn plan(&self, mut input: LlmInput) -> Result<PlannedLlm, EngineError> {
        if input.anchor.session_id.is_none() {
            return Err(EngineError::MissingSessionId);
        }
        if input.schema_v == 0 {
            input.schema_v = input.anchor.schema_v;
        }

        let prompt: PromptBundle = self
            .planner
            .build_prompt(&input, &self.config.system_prompt);
        let decision = self.router.select_model(self.client, &input)?;
        let selected_model: ModelProfile = decision.selected.clone();
        let plan =
            self.planner
                .build_plan(&input, &selected_model, self.config.planner_hint.as_ref());
        let intent = self
            .orchestrator
            .prepare_intent(&input, &plan, &selected_model, &prompt);

        Ok(PlannedLlm {
            input,
            plan,
            model_decision: decision,
            prompt,
            intent,
        })
    }

    pub fn compose_execution(
        &self,
        planned: &PlannedLlm,
        result: LlmResult,
    ) -> Result<ExecutionContext, EngineError> {
        self.orchestrator.compose_context(&planned.intent, result)
    }

    pub fn finalize(
        &self,
        planned: PlannedLlm,
        context: ExecutionContext,
    ) -> Result<LlmOutput, EngineError> {
        let ExplainPackage {
            explain,
            final_event,
        } = self.explainer.build(&planned, &context);

        let model = planned.model_decision.selected.clone();
        let plan = planned.plan.clone();

        let ExecutionContext {
            intent: _,
            result,
            degradation_chain: _,
            indices_used: _,
            query_hash: _,
        } = context;

        Ok(LlmOutput {
            schema_v: plan.schema_v,
            lineage: plan.lineage.clone(),
            final_event,
            plan,
            model,
            result,
            explain,
        })
    }
}

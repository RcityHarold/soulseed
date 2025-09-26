use crate::dto::{LlmInput, LlmOutput, ModelProfile, PromptBundle};
use crate::errors::EngineError;
use crate::explainer::{ExplainPackage, LlmExplainer};
use crate::orchestrator::{ExecutionOutcome, LlmOrchestrator};
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

    pub fn run(&self, input: LlmInput) -> Result<LlmOutput, EngineError> {
        if input.anchor.session_id.is_none() {
            return Err(EngineError::MissingSessionId);
        }

        let prompt: PromptBundle = self
            .planner
            .build_prompt(&input, &self.config.system_prompt);
        let model: ModelProfile = self.router.select_model(self.client, &input)?;
        let plan = self
            .planner
            .build_plan(&input, &model, self.config.planner_hint.as_ref());
        let outcome: ExecutionOutcome =
            self.orchestrator
                .execute(self.client, &input, &plan, &model, &prompt)?;
        let ExplainPackage {
            explain,
            final_event,
        } = self.explainer.build(&input, &model, &outcome);

        Ok(LlmOutput {
            final_event,
            plan,
            model,
            result: outcome.result.clone(),
            explain,
        })
    }
}

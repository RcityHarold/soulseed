use crate::config::{RouterConfig, ToolsConfig};
use crate::dto::{
    EngineInput, EngineOutput, OrchestratorResult, PlannerContext, RouterOutput, RouterRequest,
    RouterState, ToolPlan,
};
use crate::errors::{EngineError, OrchestratorError, PlannerError};
use crate::metrics::Observability;
use crate::tw_client::ThinWaistClient;

pub trait Router: Send + Sync {
    fn route(&self, req: &RouterRequest, state: &RouterState, cfg: &RouterConfig) -> RouterOutput;
}

pub trait Planner: Send + Sync {
    fn build_plan(&self, ctx: PlannerContext, cfg: &ToolsConfig) -> Result<ToolPlan, PlannerError>;
}

pub trait Orchestrator: Send + Sync {
    fn execute_plan(
        &self,
        plan: &ToolPlan,
        obs: &dyn Observability,
    ) -> Result<OrchestratorResult, OrchestratorError>;
}

pub trait Explainer: Send + Sync {
    fn decorate(&self, plan: &ToolPlan, result: &mut OrchestratorResult);
}

pub trait ToolEngineRunner: Send + Sync {
    fn run(
        &self,
        input: EngineInput,
        client: &dyn ThinWaistClient,
        obs: &dyn Observability,
    ) -> Result<EngineOutput, EngineError>;
}

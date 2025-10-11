use crate::dto::{LlmInput, ModelRoutingDecision};
use crate::errors::EngineError;
use crate::tw_client::ThinWaistClient;

#[derive(Clone, Default)]
pub struct LlmRouter;

impl LlmRouter {
    pub fn select_model<C: ThinWaistClient>(
        &self,
        client: &C,
        input: &LlmInput,
    ) -> Result<ModelRoutingDecision, EngineError> {
        let hints = vec![input.scene.clone()];
        Ok(client.select_model(&input.anchor, &input.scene, &hints)?)
    }
}

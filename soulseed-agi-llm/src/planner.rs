use crate::dto::{
    LlmInput, LlmPlan, LlmPlanStep, ModelProfile, PromptBundle, PromptSegment, new_plan_id,
};
use soulseed_agi_tools::dto::ToolResultSummary;

#[derive(Clone, Default)]
pub struct LlmPlanner;

impl LlmPlanner {
    pub fn build_plan(
        &self,
        input: &LlmInput,
        model: &ModelProfile,
        planner_hint: Option<&String>,
    ) -> LlmPlan {
        let mut steps = Vec::new();
        if input.clarify_prompt.is_some() {
            steps.push(LlmPlanStep {
                stage: "clarify".into(),
                note: input.clarify_prompt.clone(),
            });
        }
        if let Some(ToolResultSummary { tool_id, .. }) = &input.tool_summary {
            steps.push(LlmPlanStep {
                stage: "tool_summary".into(),
                note: Some(tool_id.clone()),
            });
        }
        steps.push(LlmPlanStep {
            stage: "llm_execute".into(),
            note: Some(model.model_id.clone()),
        });
        steps.push(LlmPlanStep {
            stage: "finalize".into(),
            note: None,
        });

        LlmPlan {
            plan_id: new_plan_id(&input.anchor),
            anchor: input.anchor.clone(),
            schema_v: input.schema_v,
            lineage: input.lineage.clone(),
            steps,
            model_hint: planner_hint.cloned(),
        }
    }

    pub fn build_prompt(&self, input: &LlmInput, system_prompt: &str) -> PromptBundle {
        let mut conversation = Vec::new();
        if let Some(clarify) = &input.clarify_prompt {
            conversation.push(PromptSegment {
                role: "clarify".into(),
                content: clarify.clone(),
            });
        }
        if let Some(summary) = input.tool_summary.as_ref() {
            conversation.push(PromptSegment {
                role: "tool".into(),
                content: format!("tool:{} summary:{}", summary.tool_id, summary.summary),
            });
        }
        conversation.push(PromptSegment {
            role: "user".into(),
            content: input.user_prompt.clone(),
        });

        PromptBundle {
            system: system_prompt.into(),
            conversation,
            tool_summary: input
                .tool_summary
                .as_ref()
                .map(|sum| serde_json::to_value(sum).unwrap_or_default()),
            metadata: input.context_tags.clone(),
        }
    }
}

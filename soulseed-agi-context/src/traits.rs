use crate::{
    config::ContextConfig,
    errors::{ContextError, QualityFailure},
    types::{
        Anchor, ContextItem, ContextScore, PlanAction, RunInput, RunOutput, ScoredItem, SummaryUnit,
    },
};

pub trait ScoreAdapter: Send + Sync {
    fn score(&self, item: &ContextItem, cfg: &ContextConfig) -> ContextScore;
}

pub trait MCCPlanner: Send + Sync {
    fn plan(
        &self,
        anchor: &Anchor,
        cfg: &ContextConfig,
        items: &[ScoredItem],
    ) -> crate::types::CompressionPlan;
}

pub struct CompressionOutcome {
    pub tokens_after: u32,
    pub summary: Option<SummaryUnit>,
}

pub trait Compressor: Send + Sync {
    fn apply(&self, item: &ContextItem, action: PlanAction) -> CompressionOutcome;
}

pub trait QualityGate: Send + Sync {
    fn evaluate(&self, summary: &SummaryUnit) -> Result<(), QualityFailure>;
}

pub trait PointerValidator: Send + Sync {
    fn validate(&self, pointer: &crate::types::EvidencePointer) -> Result<(), ContextError>;
}

pub trait ContextStore: Send + Sync {
    fn record_plan(&self, plan: &crate::types::CompressionPlan);
    fn record_report(&self, report: &crate::types::CompressionReport);
    fn record_summary(&self, summary: &SummaryUnit);
}

pub trait Observability: Send + Sync {
    fn emit_metric(&self, name: &str, value: f64, tags: &[(&str, String)]);
}

pub trait ContextEngineRunner {
    fn run(&self, input: RunInput) -> Result<RunOutput, ContextError>;
}

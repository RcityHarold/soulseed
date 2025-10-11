use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use thiserror::Error;

use crate::dto::{
    LlmResult, ModelCandidate, ModelProfile, ModelRoutingDecision, PromptBundle,
    ReasoningVisibility,
};
use soulseed_agi_tools::dto::Anchor;

#[derive(Error, Debug, Clone)]
pub enum ThinWaistError {
    #[error("llm thin waist failure: {0}")]
    Failure(String),
}

/// Bridge trait between AGI planning层 and Soulbase Thin Waist执行层。
/// 由供应商适配层实现，负责将规划请求落地到实际供应商接口。
pub trait ThinWaistClient: Send + Sync {
    fn select_model(
        &self,
        anchor: &Anchor,
        scene: &str,
        hints: &[String],
    ) -> Result<ModelRoutingDecision, ThinWaistError>;

    fn execute(
        &self,
        anchor: &Anchor,
        prompt: &PromptBundle,
        model: &ModelProfile,
    ) -> Result<LlmResult, ThinWaistError>;

    fn reconcile(&self, anchor: &Anchor, result: &LlmResult) -> Result<(), ThinWaistError>;
}

#[derive(Clone)]
pub struct MockThinWaistClient {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    decision: ModelRoutingDecision,
    results: VecDeque<Result<LlmResult, ThinWaistError>>,
    reconciles: Vec<Value>,
}

impl MockThinWaistClient {
    pub fn new(selected: ModelProfile) -> Self {
        Self::with_decision(ModelRoutingDecision {
            selected,
            candidates: Vec::new(),
            policy_trace: Value::Null,
        })
    }

    pub fn with_decision(decision: ModelRoutingDecision) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                decision,
                results: VecDeque::new(),
                reconciles: Vec::new(),
            })),
        }
    }

    pub fn push_result(&self, result: Result<LlmResult, ThinWaistError>) {
        self.inner.lock().unwrap().results.push_back(result);
    }

    pub fn take_reconciles(&self) -> Vec<Value> {
        std::mem::take(&mut self.inner.lock().unwrap().reconciles)
    }

    pub fn decision(&self) -> ModelRoutingDecision {
        self.inner.lock().unwrap().decision.clone()
    }

    pub fn set_candidates(&self, candidates: Vec<ModelCandidate>) {
        self.inner.lock().unwrap().decision.candidates = candidates;
    }

    pub fn set_policy_trace(&self, trace: Value) {
        self.inner.lock().unwrap().decision.policy_trace = trace;
    }
}

impl ThinWaistClient for MockThinWaistClient {
    fn select_model(
        &self,
        _anchor: &Anchor,
        _scene: &str,
        _hints: &[String],
    ) -> Result<ModelRoutingDecision, ThinWaistError> {
        Ok(self.inner.lock().unwrap().decision.clone())
    }

    fn execute(
        &self,
        _anchor: &Anchor,
        prompt: &PromptBundle,
        model: &ModelProfile,
    ) -> Result<LlmResult, ThinWaistError> {
        let mut inner = self.inner.lock().unwrap();
        let next = inner.results.pop_front().unwrap_or_else(|| {
            Ok(LlmResult {
                completion: format!("Model {} fallback response", model.model_id),
                summary: Some(format!("fallback summary for {}", model.model_id)),
                evidence_pointer: None,
                provider_metadata: Value::Null,
                reasoning: Vec::new(),
                reasoning_visibility: ReasoningVisibility::SummaryOnly,
                redacted: true,
                degradation_reason: None,
                indices_used: None,
                query_hash: None,
                usage: Default::default(),
                lineage: Default::default(),
            })
        });
        if let Ok(res) = &next {
            inner
                .reconciles
                .push(serde_json::to_value(prompt).unwrap_or_default());
            inner
                .reconciles
                .push(serde_json::json!({ "result": res.completion }));
        }
        next
    }

    fn reconcile(&self, _anchor: &Anchor, result: &LlmResult) -> Result<(), ThinWaistError> {
        self.inner
            .lock()
            .unwrap()
            .reconciles
            .push(serde_json::json!({"completion": result.completion}));
        Ok(())
    }
}

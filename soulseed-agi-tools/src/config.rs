use serde::{Deserialize, Serialize};

use crate::dto::{
    Budget, CircuitBreaker, DegradeRule, FallbackRule, OrchestrationMode, OrchestrationStrategy,
    RetryBackoff,
};
use crate::errors::ConfigError;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterWeights {
    pub context_fit: f32,
    pub success_rate: f32,
    pub latency_p95: f32,
    pub cost_per_success: f32,
    pub risk_level: f32,
    pub auth_impact: f32,
    pub cacheability: f32,
    pub degradation_acceptance: f32,
}

impl Default for RouterWeights {
    fn default() -> Self {
        Self {
            context_fit: 0.30,
            success_rate: 0.20,
            latency_p95: -0.10,
            cost_per_success: -0.15,
            risk_level: -0.15,
            auth_impact: -0.05,
            cacheability: 0.10,
            degradation_acceptance: 0.05,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RouterConfig {
    pub weights: RouterWeights,
    pub explore_rate: f32,
    pub risk_threshold: String,
    pub max_candidates: usize,
    pub plan_seed: u64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            weights: RouterWeights::default(),
            explore_rate: 0.05,
            risk_threshold: "medium".into(),
            max_candidates: 3,
            plan_seed: 42,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OrchestratorConfig {
    pub mode: OrchestrationMode,
    pub retry: RetryBackoff,
    pub timeout_ms: u32,
    pub deadline_ms: Option<u64>,
    pub circuit: CircuitBreaker,
    pub fallback: Vec<FallbackRule>,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            mode: OrchestrationMode::Hybrid,
            retry: RetryBackoff {
                max_retries: 2,
                base_ms: 200,
                jitter: true,
            },
            timeout_ms: 300,
            deadline_ms: Some(1500),
            circuit: CircuitBreaker {
                open_after: 10,
                half_open_after: 5000,
                error_ratio: 0.5,
            },
            fallback: vec![
                FallbackRule {
                    on: "QOS.RATE_LIMITED".into(),
                    to: "web.search.summary".into(),
                },
                FallbackRule {
                    on: "TOOL.TIMEOUT".into(),
                    to: "cache.only".into(),
                },
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetConfig {
    pub max_cost_tokens: u64,
    pub max_latency_ms: u32,
    pub qos_policy: String,
    pub degrade_rules: Vec<DegradeRule>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_cost_tokens: 8000,
            max_latency_ms: 1500,
            qos_policy: "burst-allow-5-per-sec".into(),
            degrade_rules: vec![DegradeRule {
                from: "web.search.full".into(),
                to: "web.search.summary".into(),
                reason: "budget".into(),
            }],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolsConfig {
    pub snapshot_hash: String,
    pub snapshot_version: u32,
    pub router: RouterConfig,
    pub orchestrator: OrchestratorConfig,
    pub budget: BudgetConfig,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            snapshot_hash: "cfg-tools-default".into(),
            snapshot_version: 1,
            router: RouterConfig::default(),
            orchestrator: OrchestratorConfig::default(),
            budget: BudgetConfig::default(),
        }
    }
}

impl ToolsConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.router.explore_rate < 0.0 || self.router.explore_rate > 1.0 {
            return Err(ConfigError::InvalidField("router.explore_rate".into()));
        }
        if self.router.max_candidates == 0 {
            return Err(ConfigError::InvalidField("router.max_candidates".into()));
        }
        if !(0.0..=1.0).contains(&self.orchestrator.circuit.error_ratio) {
            return Err(ConfigError::InvalidField(
                "orchestrator.circuit.error_ratio".into(),
            ));
        }
        if self.orchestrator.timeout_ms == 0 {
            return Err(ConfigError::InvalidField("orchestrator.timeout_ms".into()));
        }
        if self.budget.max_latency_ms == 0 {
            return Err(ConfigError::InvalidField("budget.max_latency_ms".into()));
        }
        if self.budget.max_cost_tokens == 0 {
            return Err(ConfigError::InvalidField("budget.max_cost_tokens".into()));
        }
        Ok(())
    }

    pub fn to_budget(&self) -> Budget {
        Budget {
            max_cost_tokens: self.budget.max_cost_tokens,
            max_latency_ms: self.budget.max_latency_ms,
            qos_policy: self.budget.qos_policy.clone(),
            degrade_rules: self.budget.degrade_rules.clone(),
        }
    }

    pub fn to_strategy(&self) -> OrchestrationStrategy {
        OrchestrationStrategy {
            mode: self.orchestrator.mode,
            retry: self.orchestrator.retry.clone(),
            timeout_ms: self.orchestrator.timeout_ms,
            deadline_ms: self.orchestrator.deadline_ms,
            circuit: self.orchestrator.circuit.clone(),
            fallback: self.orchestrator.fallback.clone(),
        }
    }
}

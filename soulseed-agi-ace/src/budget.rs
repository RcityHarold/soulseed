use crate::errors::AceError;
use crate::types::{BudgetDecision, BudgetSnapshot, CycleLane};

#[derive(Clone, Debug, Default)]
pub struct BudgetPolicy {
    pub lane_token_ceiling: Option<u32>,
    pub lane_walltime_ceiling_ms: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct BudgetManager {
    pub clarify_policy: BudgetPolicy,
    pub tool_policy: BudgetPolicy,
    pub self_policy: BudgetPolicy,
    pub collab_policy: BudgetPolicy,
}

impl BudgetManager {
    fn policy_for(&self, lane: &CycleLane) -> &BudgetPolicy {
        match lane {
            CycleLane::Clarify => &self.clarify_policy,
            CycleLane::Tool => &self.tool_policy,
            CycleLane::SelfReason => &self.self_policy,
            CycleLane::Collab => &self.collab_policy,
        }
    }

    pub fn evaluate(
        &self,
        cycle_id: soulseed_agi_core_models::CycleId,
        lane: &CycleLane,
        snapshot: BudgetSnapshot,
    ) -> Result<BudgetDecision, AceError> {
        let policy = self.policy_for(lane);
        if let Some(max_tokens) = policy.lane_token_ceiling {
            if snapshot.tokens_spent > max_tokens {
                return Ok(BudgetDecision {
                    cycle_id,
                    allowed: false,
                    reason: Some("token_budget_exceeded".into()),
                    snapshot,
                });
            }
        }
        if let Some(max_ms) = policy.lane_walltime_ceiling_ms {
            if snapshot.walltime_ms_used > max_ms {
                return Ok(BudgetDecision {
                    cycle_id,
                    allowed: false,
                    reason: Some("walltime_budget_exceeded".into()),
                    snapshot,
                });
            }
        }
        Ok(BudgetDecision {
            cycle_id,
            allowed: true,
            reason: None,
            snapshot,
        })
    }
}

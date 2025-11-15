use crate::errors::AceError;
use crate::types::{BudgetDecision, BudgetSnapshot, CycleLane};
use serde::{Deserialize, Serialize};

/// 预算降级策略 - 当预算接近或超过限制时的优雅降级选项
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DegradationStrategy {
    /// 提供保守答案 (最低成本)
    Conservative,
    /// 转工具执行 (中等成本，可能更准确)
    TransferToTool,
    /// 转协同执行 (较高成本，多Agent协同)
    TransferToCollab,
    /// 请求人工决策 (HITL)
    AskHumanDecision,
    /// 暂停等待预算补充
    Pause,
    /// 拒绝执行 (完全拒绝)
    Reject,
}

#[derive(Clone, Debug)]
pub struct BudgetPolicy {
    pub lane_token_ceiling: Option<u32>,
    pub lane_walltime_ceiling_ms: Option<u64>,
    pub lane_external_cost_ceiling: Option<f32>,

    // 降级阈值 (百分比，0.0-1.0)
    /// 当使用量超过此阈值时触发Conservative降级
    pub conservative_threshold: f32,
    /// 当使用量超过此阈值时触发TransferToTool降级
    pub tool_threshold: f32,
    /// 当使用量超过此阈值时触发TransferToCollab降级
    pub collab_threshold: f32,
    /// 当使用量超过此阈值时触发AskHumanDecision
    pub hitl_threshold: f32,
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self {
            lane_token_ceiling: None,
            lane_walltime_ceiling_ms: None,
            lane_external_cost_ceiling: None,
            // 默认降级阈值:
            // 60% - 提供保守答案
            // 75% - 转工具执行
            // 85% - 转协同
            // 95% - 请求人工决策
            conservative_threshold: 0.60,
            tool_threshold: 0.75,
            collab_threshold: 0.85,
            hitl_threshold: 0.95,
        }
    }
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

    /// 计算预算使用率 (返回0.0-1.0+的比率，可能超过1.0)
    fn calculate_usage_ratio(&self, snapshot: &BudgetSnapshot) -> f32 {
        let mut max_ratio = 0.0_f32;

        // Token使用率
        if snapshot.tokens_allowed > 0 {
            let token_ratio = snapshot.tokens_spent as f32 / snapshot.tokens_allowed as f32;
            max_ratio = max_ratio.max(token_ratio);
        }

        // 时间使用率
        if snapshot.walltime_ms_allowed > 0 {
            let time_ratio = snapshot.walltime_ms_used as f32 / snapshot.walltime_ms_allowed as f32;
            max_ratio = max_ratio.max(time_ratio);
        }

        // 外部成本使用率
        if snapshot.external_cost_allowed > 0.0 {
            let cost_ratio = snapshot.external_cost_spent / snapshot.external_cost_allowed;
            max_ratio = max_ratio.max(cost_ratio);
        }

        max_ratio
    }

    /// 根据预算使用率评估降级策略
    pub fn evaluate_degradation(
        &self,
        lane: &CycleLane,
        snapshot: &BudgetSnapshot,
    ) -> Option<DegradationStrategy> {
        let policy = self.policy_for(lane);
        let usage_ratio = self.calculate_usage_ratio(snapshot);

        // 如果完全超出预算，拒绝执行
        if usage_ratio >= 1.0 {
            return Some(DegradationStrategy::Reject);
        }

        // 根据阈值选择降级策略
        if usage_ratio >= policy.hitl_threshold {
            Some(DegradationStrategy::AskHumanDecision)
        } else if usage_ratio >= policy.collab_threshold {
            // 如果当前已经是Collab lane，则降级到Pause
            if matches!(lane, CycleLane::Collab) {
                Some(DegradationStrategy::Pause)
            } else {
                Some(DegradationStrategy::TransferToCollab)
            }
        } else if usage_ratio >= policy.tool_threshold {
            // 如果当前已经是Tool lane，则降级到Conservative
            if matches!(lane, CycleLane::Tool) {
                Some(DegradationStrategy::Conservative)
            } else {
                Some(DegradationStrategy::TransferToTool)
            }
        } else if usage_ratio >= policy.conservative_threshold {
            Some(DegradationStrategy::Conservative)
        } else {
            // 预算充足，无需降级
            None
        }
    }

    pub fn evaluate(
        &self,
        cycle_id: soulseed_agi_core_models::AwarenessCycleId,
        lane: &CycleLane,
        snapshot: BudgetSnapshot,
    ) -> Result<BudgetDecision, AceError> {
        let policy = self.policy_for(lane);
        let usage_ratio = self.calculate_usage_ratio(&snapshot);

        // 评估降级策略
        let degradation_strategy = self.evaluate_degradation(lane, &snapshot);

        // 检查是否完全超出预算
        let mut exceeded = false;
        let mut exceeded_reason = None;
        let mut degradation_reason_str = None;

        if let Some(max_tokens) = policy.lane_token_ceiling {
            if snapshot.tokens_spent > max_tokens {
                exceeded = true;
                exceeded_reason = Some("token_budget_exceeded".into());
                degradation_reason_str = Some("budget_tokens".into());
            }
        }
        if let Some(max_ms) = policy.lane_walltime_ceiling_ms {
            if snapshot.walltime_ms_used > max_ms {
                exceeded = true;
                exceeded_reason = Some("walltime_budget_exceeded".into());
                degradation_reason_str = Some("budget_walltime".into());
            }
        }
        if let Some(max_cost) = policy.lane_external_cost_ceiling {
            if snapshot.external_cost_spent > max_cost {
                exceeded = true;
                exceeded_reason = Some("external_cost_budget_exceeded".into());
                degradation_reason_str = Some("budget_external_cost".into());
            }
        }

        // 如果完全超出预算，拒绝执行
        if exceeded {
            return Ok(BudgetDecision {
                cycle_id,
                allowed: false,
                reason: exceeded_reason,
                snapshot,
                degradation_reason: degradation_reason_str,
                degradation_strategy: Some(DegradationStrategy::Reject),
            });
        }

        // 如果有降级策略但预算未完全耗尽，仍允许执行但建议降级
        if let Some(strategy) = &degradation_strategy {
            tracing::info!(
                "Budget usage at {:.1}% for lane {:?}, suggesting degradation: {:?}",
                usage_ratio * 100.0,
                lane,
                strategy
            );
        }

        Ok(BudgetDecision {
            cycle_id,
            allowed: true,
            reason: None,
            snapshot,
            degradation_reason: degradation_strategy.as_ref().map(|s| format!("{:?}", s)),
            degradation_strategy,
        })
    }
}

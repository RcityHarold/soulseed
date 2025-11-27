//! 决策审计器
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! DecisionAuditor 负责审计决策过程，分析决策理由和备选方案

use super::types::*;
use serde::{Deserialize, Serialize};

/// 决策审计器
///
/// 提供决策审计功能：
/// - audit_decision: 审计单个决策
/// - compare_alternatives: 比较备选方案
/// - get_decision_rationale: 获取决策理由
pub struct DecisionAuditor {
    /// 是否包含详细分析
    include_detailed_analysis: bool,
}

impl DecisionAuditor {
    pub fn new() -> Self {
        Self {
            include_detailed_analysis: true,
        }
    }

    pub fn with_detailed_analysis(mut self, include: bool) -> Self {
        self.include_detailed_analysis = include;
        self
    }

    /// 审计单个决策
    pub async fn audit_decision(&self, decision_event_id: &str) -> Option<DecisionAuditReport> {
        // 示例 SurrealQL:
        // ```
        // SELECT
        //   event_id,
        //   payload.decision_path as decision_path,
        //   payload.confidence as confidence,
        //   payload.rationale as rationale,
        //   payload.alternatives_considered as alternatives
        // FROM event
        // WHERE id = $decision_event_id
        //   AND event_type = 'decision_routed'
        // ```

        // 框架实现
        Some(DecisionAuditReport {
            decision_event_id: decision_event_id.to_string(),
            decision_path: "self_reason".to_string(),
            confidence: 0.85,
            rationale: "High clarity score, no external dependencies needed".to_string(),
            alternatives: vec![
                AlternativeAnalysis {
                    path: "clarify".to_string(),
                    score: 0.3,
                    rejection_reason: "User intent was clear".to_string(),
                    potential_outcome: Some("Would have delayed response".to_string()),
                },
                AlternativeAnalysis {
                    path: "tool_path".to_string(),
                    score: 0.4,
                    rejection_reason: "No tools required for this query".to_string(),
                    potential_outcome: None,
                },
            ],
            context_factors: vec![
                ContextFactor {
                    factor_name: "clarity_score".to_string(),
                    factor_value: serde_json::json!(0.92),
                    influence_weight: 0.4,
                },
                ContextFactor {
                    factor_name: "external_need".to_string(),
                    factor_value: serde_json::json!(false),
                    influence_weight: 0.3,
                },
            ],
            outcome_assessment: None,
        })
    }

    /// 比较备选方案
    pub async fn compare_alternatives(
        &self,
        decision_event_id: &str,
    ) -> AlternativesAnalysisResult {
        let audit = self.audit_decision(decision_event_id).await;

        if let Some(report) = audit {
            AlternativesAnalysisResult {
                decision_event_id: decision_event_id.to_string(),
                chosen_path: report.decision_path.clone(),
                alternatives: report.alternatives,
                analysis_summary: format!(
                    "Decision '{}' was chosen with confidence {:.2}",
                    report.decision_path, report.confidence
                ),
                could_improve: report.confidence < 0.7,
                improvement_suggestions: if report.confidence < 0.7 {
                    vec!["Consider gathering more context".to_string()]
                } else {
                    vec![]
                },
            }
        } else {
            AlternativesAnalysisResult {
                decision_event_id: decision_event_id.to_string(),
                chosen_path: "unknown".to_string(),
                alternatives: vec![],
                analysis_summary: "Decision not found".to_string(),
                could_improve: false,
                improvement_suggestions: vec![],
            }
        }
    }

    /// 获取决策理由
    pub async fn get_decision_rationale(&self, decision_event_id: &str) -> Option<Rationale> {
        // 框架实现
        Some(Rationale {
            decision_event_id: decision_event_id.to_string(),
            primary_reason: "Clear user intent with no external dependencies".to_string(),
            supporting_factors: vec![
                "High clarity score (0.92)".to_string(),
                "No tool requirements detected".to_string(),
                "Context sufficient for response".to_string(),
            ],
            confidence_factors: vec![
                ConfidenceFactor {
                    name: "intent_clarity".to_string(),
                    value: 0.92,
                    contribution: 0.4,
                },
                ConfidenceFactor {
                    name: "context_completeness".to_string(),
                    value: 0.85,
                    contribution: 0.3,
                },
            ],
            risk_assessment: RiskAssessment {
                overall_risk: RiskLevel::Low,
                potential_issues: vec![],
                mitigation_strategies: vec![],
            },
        })
    }

    /// 批量审计决策
    pub async fn batch_audit(
        &self,
        decision_event_ids: &[String],
    ) -> Vec<Option<DecisionAuditReport>> {
        let mut results = Vec::with_capacity(decision_event_ids.len());
        for id in decision_event_ids {
            results.push(self.audit_decision(id).await);
        }
        results
    }

    /// 获取决策历史统计
    pub async fn get_decision_statistics(
        &self,
        time_range: &TimeRange,
    ) -> DecisionStatistics {
        // 示例 SurrealQL:
        // ```
        // SELECT
        //   payload.decision_path as path,
        //   count() as count,
        //   math::mean(payload.confidence) as avg_confidence
        // FROM event
        // WHERE event_type = 'decision_routed'
        //   AND occurred_at_ms >= $start
        //   AND occurred_at_ms <= $end
        // GROUP BY path
        // ```

        // 框架实现
        DecisionStatistics {
            time_range: time_range.clone(),
            total_decisions: 100,
            path_distribution: vec![
                PathCount {
                    path: "self_reason".to_string(),
                    count: 60,
                    percentage: 60.0,
                },
                PathCount {
                    path: "clarify".to_string(),
                    count: 20,
                    percentage: 20.0,
                },
                PathCount {
                    path: "tool_path".to_string(),
                    count: 15,
                    percentage: 15.0,
                },
                PathCount {
                    path: "collab".to_string(),
                    count: 5,
                    percentage: 5.0,
                },
            ],
            average_confidence: 0.82,
            low_confidence_count: 8,
            rejection_rate: 0.03,
        }
    }

    /// 评估决策结果
    pub async fn assess_outcome(
        &self,
        decision_event_id: &str,
        was_successful: bool,
        actual_cost: f64,
        actual_duration_ms: u64,
        user_satisfaction: Option<f32>,
    ) -> OutcomeAssessment {
        OutcomeAssessment {
            was_successful,
            actual_cost,
            actual_duration_ms,
            user_satisfaction,
            notes: Some(format!(
                "Assessment for decision {}",
                decision_event_id
            )),
        }
    }
}

impl Default for DecisionAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// 备选方案分析结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlternativesAnalysisResult {
    pub decision_event_id: String,
    pub chosen_path: String,
    pub alternatives: Vec<AlternativeAnalysis>,
    pub analysis_summary: String,
    pub could_improve: bool,
    pub improvement_suggestions: Vec<String>,
}

/// 决策理由
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rationale {
    pub decision_event_id: String,
    pub primary_reason: String,
    pub supporting_factors: Vec<String>,
    pub confidence_factors: Vec<ConfidenceFactor>,
    pub risk_assessment: RiskAssessment,
}

/// 置信度因素
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfidenceFactor {
    pub name: String,
    pub value: f32,
    pub contribution: f32,
}

/// 风险评估
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub overall_risk: RiskLevel,
    pub potential_issues: Vec<String>,
    pub mitigation_strategies: Vec<String>,
}

/// 风险级别
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// 决策统计
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionStatistics {
    pub time_range: TimeRange,
    pub total_decisions: u32,
    pub path_distribution: Vec<PathCount>,
    pub average_confidence: f32,
    pub low_confidence_count: u32,
    pub rejection_rate: f32,
}

/// 路径计数
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathCount {
    pub path: String,
    pub count: u32,
    pub percentage: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_decision() {
        let auditor = DecisionAuditor::new();
        let report = auditor.audit_decision("decision-1").await;
        assert!(report.is_some());
        let report = report.unwrap();
        assert_eq!(report.decision_event_id, "decision-1");
        assert!(report.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_compare_alternatives() {
        let auditor = DecisionAuditor::new();
        let result = auditor.compare_alternatives("decision-1").await;
        assert_eq!(result.decision_event_id, "decision-1");
        assert!(!result.alternatives.is_empty());
    }

    #[tokio::test]
    async fn test_get_decision_rationale() {
        let auditor = DecisionAuditor::new();
        let rationale = auditor.get_decision_rationale("decision-1").await;
        assert!(rationale.is_some());
        let rationale = rationale.unwrap();
        assert!(!rationale.primary_reason.is_empty());
    }
}

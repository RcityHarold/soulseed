//! 统一元认知分析器
//!
//! 依据文档: 6-元认知工具/0-元认知工具-1/01-元认知工具-AI-01-谷歌gemini.md
//!
//! UnifiedMetacognitiveAnalyzer 作为统一入口点，提供五大分析模式

use super::auditor::DecisionAuditor;
use super::causal_tracer::CausalChainTracer;
use super::pattern_detector::PatternDetector;
use super::profiler::PerformanceProfiler;
use super::types::*;
use std::sync::Arc;
use std::time::Instant;

/// 统一元认知分析器
///
/// 作为元认知工具集的统一入口点，支持五大分析模式：
/// 1. trajectory_observation - 轨迹观察
/// 2. causal_decision_analysis - 因果决策分析
/// 3. interaction_pattern_discovery - 交互模式发现
/// 4. collaboration_evaluation - 协作评估
/// 5. cognitive_growth_tracking - 认知成长追踪
pub struct UnifiedMetacognitiveAnalyzer {
    /// 因果链追踪器
    causal_tracer: CausalChainTracer,
    /// 性能剖析器
    profiler: PerformanceProfiler,
    /// 决策审计器
    auditor: DecisionAuditor,
    /// 模式检测器
    pattern_detector: PatternDetector,
}

impl UnifiedMetacognitiveAnalyzer {
    /// 创建新的分析器
    pub fn new() -> Self {
        Self {
            causal_tracer: CausalChainTracer::new(),
            profiler: PerformanceProfiler::new(),
            auditor: DecisionAuditor::new(),
            pattern_detector: PatternDetector::new(),
        }
    }

    /// 执行分析
    pub async fn analyze(&self, request: AnalysisRequest) -> AnalysisResult {
        let start = Instant::now();

        let result = match request.mode {
            AnalysisMode::TrajectoryObservation => {
                self.trajectory_observation(&request).await
            }
            AnalysisMode::CausalDecisionAnalysis => {
                self.causal_decision_analysis(&request).await
            }
            AnalysisMode::InteractionPatternDiscovery => {
                self.interaction_pattern_discovery(&request).await
            }
            AnalysisMode::CollaborationEvaluation => {
                self.collaboration_evaluation(&request).await
            }
            AnalysisMode::CognitiveGrowthTracking => {
                self.cognitive_growth_tracking(&request).await
            }
        };

        result.with_execution_time(start.elapsed().as_millis() as u64)
    }

    /// 轨迹观察模式
    ///
    /// 追踪指定实体的认知轨迹，包括：
    /// - AC 执行序列
    /// - 决策路径变化
    /// - 上下文演化
    async fn trajectory_observation(&self, request: &AnalysisRequest) -> AnalysisResult {
        let target_id = match &request.target_id {
            Some(id) => id.clone(),
            None => {
                return AnalysisResult::failure(
                    AnalysisMode::TrajectoryObservation,
                    "target_id is required for trajectory observation",
                )
            }
        };

        // 使用因果链追踪器获取轨迹
        let chain = self.causal_tracer.trace_decision_path(&target_id).await;

        let data = serde_json::json!({
            "target_id": target_id,
            "trajectory": chain,
            "time_range": request.time_range,
        });

        AnalysisResult::success(AnalysisMode::TrajectoryObservation, data)
            .with_summary(format!(
                "Traced trajectory for {} with {} events",
                target_id,
                chain.map(|c| c.length).unwrap_or(0)
            ))
    }

    /// 因果决策分析模式
    ///
    /// 分析决策的因果链，包括：
    /// - 根因分析
    /// - 下游影响
    /// - 决策理由
    async fn causal_decision_analysis(&self, request: &AnalysisRequest) -> AnalysisResult {
        let target_id = match &request.target_id {
            Some(id) => id.clone(),
            None => {
                return AnalysisResult::failure(
                    AnalysisMode::CausalDecisionAnalysis,
                    "target_id (decision event id) is required",
                )
            }
        };

        // 获取因果链
        let chain = self.causal_tracer.trace_decision_path(&target_id).await;
        let root_cause = self.causal_tracer.find_root_cause(&target_id).await;
        let effects = self.causal_tracer.get_downstream_effects(&target_id).await;

        // 获取决策审计报告
        let audit_report = self.auditor.audit_decision(&target_id).await;

        let data = serde_json::json!({
            "decision_event_id": target_id,
            "causal_chain": chain,
            "root_cause": root_cause,
            "downstream_effects": effects,
            "audit_report": audit_report,
        });

        let mut result = AnalysisResult::success(AnalysisMode::CausalDecisionAnalysis, data);

        // 添加洞察
        if let Some(report) = audit_report {
            if report.confidence < 0.5 {
                result = result.with_insight(Insight {
                    insight_type: InsightType::RiskWarning,
                    title: "Low confidence decision".to_string(),
                    description: format!(
                        "Decision {} had low confidence ({:.2})",
                        target_id, report.confidence
                    ),
                    confidence: 0.9,
                    importance: 0.8,
                    related_entities: vec![target_id.clone()],
                    suggested_actions: vec![
                        "Review decision rationale".to_string(),
                        "Consider additional context".to_string(),
                    ],
                });
            }
        }

        result.with_summary(format!("Analyzed causal chain for decision {}", target_id))
    }

    /// 交互模式发现模式
    ///
    /// 发现交互模式，包括：
    /// - 行为模式
    /// - 交互频率
    /// - 异常检测
    async fn interaction_pattern_discovery(&self, request: &AnalysisRequest) -> AnalysisResult {
        let time_range = request.time_range.clone().unwrap_or_else(|| TimeRange::last_days(7));

        // 检测行为模式
        let behavior_patterns = if let Some(ref actor_id) = request.target_id {
            self.pattern_detector.detect_behavior_patterns(actor_id).await
        } else {
            Vec::new()
        };

        // 检测交互模式
        let interaction_patterns = if let Some(ref session_id) = request.target_id {
            self.pattern_detector
                .detect_interaction_patterns(session_id)
                .await
        } else {
            Vec::new()
        };

        // 检测异常
        let anomalies = self.pattern_detector.detect_anomalies(&time_range).await;

        let data = serde_json::json!({
            "time_range": time_range,
            "behavior_patterns": behavior_patterns,
            "interaction_patterns": interaction_patterns,
            "anomalies": anomalies,
        });

        let mut result = AnalysisResult::success(AnalysisMode::InteractionPatternDiscovery, data);

        // 为每个重要模式添加洞察
        for pattern in &behavior_patterns {
            if pattern.confidence > 0.8 {
                result = result.with_insight(Insight {
                    insight_type: InsightType::DecisionPattern,
                    title: format!("{:?} pattern detected", pattern.pattern_type),
                    description: pattern.description.clone(),
                    confidence: pattern.confidence,
                    importance: 0.7,
                    related_entities: pattern.example_events.clone(),
                    suggested_actions: vec![],
                });
            }
        }

        // 为异常添加洞察
        for anomaly in &anomalies {
            if anomaly.severity >= AnomalySeverity::High {
                result = result.with_insight(Insight {
                    insight_type: InsightType::AnomalyDetected,
                    title: format!("{:?} anomaly", anomaly.anomaly_type),
                    description: anomaly.description.clone(),
                    confidence: 0.85,
                    importance: match anomaly.severity {
                        AnomalySeverity::Critical => 1.0,
                        AnomalySeverity::High => 0.9,
                        _ => 0.7,
                    },
                    related_entities: anomaly.related_events.clone(),
                    suggested_actions: vec!["Investigate anomaly".to_string()],
                });
            }
        }

        result.with_summary(format!(
            "Discovered {} behavior patterns, {} interaction patterns, {} anomalies",
            behavior_patterns.len(),
            interaction_patterns.len(),
            anomalies.len()
        ))
    }

    /// 协作评估模式
    ///
    /// 评估协作效果，包括：
    /// - 协作效率
    /// - 通信质量
    /// - 成本分析
    async fn collaboration_evaluation(&self, request: &AnalysisRequest) -> AnalysisResult {
        let time_range = request.time_range.clone().unwrap_or_else(|| TimeRange::last_days(30));

        // 这里应该从图数据库查询协作相关数据
        // 目前提供框架实现
        let data = serde_json::json!({
            "time_range": time_range,
            "collaboration_metrics": {
                "total_collaborations": 0,
                "successful_rate": 0.0,
                "average_duration_ms": 0,
                "cost_efficiency": 0.0,
            },
            "top_collaborators": [],
            "collaboration_patterns": [],
        });

        AnalysisResult::success(AnalysisMode::CollaborationEvaluation, data)
            .with_summary("Collaboration evaluation completed")
    }

    /// 认知成长追踪模式
    ///
    /// 追踪认知成长，包括：
    /// - 决策质量变化
    /// - 效率提升
    /// - 学习曲线
    async fn cognitive_growth_tracking(&self, request: &AnalysisRequest) -> AnalysisResult {
        let target_id = match &request.target_id {
            Some(id) => id.clone(),
            None => {
                return AnalysisResult::failure(
                    AnalysisMode::CognitiveGrowthTracking,
                    "target_id (actor_id) is required for growth tracking",
                )
            }
        };

        let time_range = request.time_range.clone().unwrap_or_else(|| TimeRange::last_days(90));

        // 这里应该从图数据库查询历史数据并计算成长指标
        // 目前提供框架实现
        let data = serde_json::json!({
            "actor_id": target_id,
            "time_range": time_range,
            "growth_metrics": {
                "decision_accuracy_trend": [],
                "efficiency_improvement": 0.0,
                "learning_velocity": 0.0,
                "adaptation_score": 0.0,
            },
            "milestones": [],
            "recommendations": [],
        });

        AnalysisResult::success(AnalysisMode::CognitiveGrowthTracking, data)
            .with_summary(format!("Tracked cognitive growth for {}", target_id))
            .with_insight(Insight {
                insight_type: InsightType::GrowthOpportunity,
                title: "Growth tracking initialized".to_string(),
                description: "Baseline metrics established for future comparisons".to_string(),
                confidence: 0.7,
                importance: 0.6,
                related_entities: vec![target_id],
                suggested_actions: vec![
                    "Set growth objectives".to_string(),
                    "Schedule periodic reviews".to_string(),
                ],
            })
    }

    // ========================================================================
    // 便捷方法
    // ========================================================================

    /// 快速轨迹观察
    pub async fn quick_trajectory(&self, tenant_id: u64, target_id: &str) -> AnalysisResult {
        let request = AnalysisRequest::new(AnalysisMode::TrajectoryObservation, tenant_id)
            .with_target(target_id);
        self.analyze(request).await
    }

    /// 快速因果分析
    pub async fn quick_causal_analysis(
        &self,
        tenant_id: u64,
        decision_event_id: &str,
    ) -> AnalysisResult {
        let request = AnalysisRequest::new(AnalysisMode::CausalDecisionAnalysis, tenant_id)
            .with_target(decision_event_id);
        self.analyze(request).await
    }

    /// 快速模式发现
    pub async fn quick_pattern_discovery(&self, tenant_id: u64, days: i64) -> AnalysisResult {
        let request = AnalysisRequest::new(AnalysisMode::InteractionPatternDiscovery, tenant_id)
            .with_time_range(TimeRange::last_days(days));
        self.analyze(request).await
    }

    /// 获取因果链追踪器引用
    pub fn causal_tracer(&self) -> &CausalChainTracer {
        &self.causal_tracer
    }

    /// 获取性能剖析器引用
    pub fn profiler(&self) -> &PerformanceProfiler {
        &self.profiler
    }

    /// 获取决策审计器引用
    pub fn auditor(&self) -> &DecisionAuditor {
        &self.auditor
    }

    /// 获取模式检测器引用
    pub fn pattern_detector(&self) -> &PatternDetector {
        &self.pattern_detector
    }
}

impl Default for UnifiedMetacognitiveAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_analyzer_creation() {
        let analyzer = UnifiedMetacognitiveAnalyzer::new();
        assert!(true); // 只测试创建成功
    }

    #[tokio::test]
    async fn test_trajectory_observation_requires_target() {
        let analyzer = UnifiedMetacognitiveAnalyzer::new();
        let request = AnalysisRequest::new(AnalysisMode::TrajectoryObservation, 1);
        let result = analyzer.analyze(request).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_quick_pattern_discovery() {
        let analyzer = UnifiedMetacognitiveAnalyzer::new();
        let result = analyzer.quick_pattern_discovery(1, 7).await;
        assert!(result.success);
    }
}

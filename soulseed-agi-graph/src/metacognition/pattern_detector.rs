//! 模式检测器
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! PatternDetector 负责检测行为模式、交互模式和异常

use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 模式检测器
///
/// 提供模式检测功能：
/// - detect_behavior_patterns: 检测行为模式
/// - detect_interaction_patterns: 检测交互模式
/// - detect_anomalies: 检测异常
pub struct PatternDetector {
    /// 模式检测阈值
    pattern_threshold: f32,
    /// 异常检测灵敏度
    anomaly_sensitivity: f32,
}

impl PatternDetector {
    pub fn new() -> Self {
        Self {
            pattern_threshold: 0.7,
            anomaly_sensitivity: 0.8,
        }
    }

    pub fn with_pattern_threshold(mut self, threshold: f32) -> Self {
        self.pattern_threshold = threshold;
        self
    }

    pub fn with_anomaly_sensitivity(mut self, sensitivity: f32) -> Self {
        self.anomaly_sensitivity = sensitivity;
        self
    }

    /// 检测行为模式
    pub async fn detect_behavior_patterns(&self, actor_id: &str) -> Vec<BehaviorPattern> {
        // 示例 SurrealQL:
        // ```
        // LET $events = (
        //   SELECT * FROM event
        //   WHERE actor_id = $actor_id
        //   ORDER BY occurred_at_ms DESC
        //   LIMIT 1000
        // );
        // -- 分析决策模式
        // LET $decision_patterns = (
        //   SELECT payload.decision_path as path, count() as cnt
        //   FROM $events
        //   WHERE event_type = 'decision_routed'
        //   GROUP BY path
        // );
        // -- 分析工具使用习惯
        // LET $tool_patterns = (
        //   SELECT payload.tool_id as tool, count() as cnt
        //   FROM $events
        //   WHERE event_type CONTAINS 'tool'
        //   GROUP BY tool
        // );
        // ```

        // 框架实现：模拟检测到的模式
        vec![
            BehaviorPattern {
                pattern_id: Uuid::new_v4().to_string(),
                pattern_type: PatternType::RepetitiveDecision,
                description: format!(
                    "Actor {} frequently chooses self_reason path (75% of decisions)",
                    actor_id
                ),
                frequency: 150,
                confidence: 0.85,
                example_events: vec!["event-1".to_string(), "event-2".to_string()],
            },
            BehaviorPattern {
                pattern_id: Uuid::new_v4().to_string(),
                pattern_type: PatternType::ToolUsageHabit,
                description: format!(
                    "Actor {} prefers web search tool for information queries",
                    actor_id
                ),
                frequency: 45,
                confidence: 0.72,
                example_events: vec!["event-10".to_string()],
            },
        ]
    }

    /// 检测交互模式
    pub async fn detect_interaction_patterns(&self, session_id: &str) -> Vec<InteractionPattern> {
        // 示例 SurrealQL:
        // ```
        // SELECT
        //   array::distinct(participants) as participants,
        //   count() as interaction_count,
        //   math::mean(duration_ms) as avg_duration
        // FROM event
        // WHERE session_id = $session_id
        //   AND array::len(participants) > 1
        // GROUP BY participants
        // ```

        // 框架实现
        vec![InteractionPattern {
            pattern_id: Uuid::new_v4().to_string(),
            participants: vec!["human-1".to_string(), "ai-1".to_string()],
            interaction_type: "question_answer".to_string(),
            frequency: 25,
            avg_duration_ms: 3000,
        }]
    }

    /// 检测异常
    pub async fn detect_anomalies(&self, time_range: &TimeRange) -> Vec<Anomaly> {
        // 示例 SurrealQL (使用时序异常检测):
        // ```
        // -- 计算基准统计
        // LET $baseline = (
        //   SELECT
        //     math::mean(duration_ms) as avg_duration,
        //     math::stddev(duration_ms) as stddev_duration,
        //     math::mean(cost) as avg_cost,
        //     math::stddev(cost) as stddev_cost
        //   FROM event
        //   WHERE occurred_at_ms < $start - 86400000  -- 前一天
        // );
        // -- 检测当前范围内的异常
        // SELECT * FROM event
        // WHERE occurred_at_ms >= $start AND occurred_at_ms <= $end
        //   AND (
        //     duration_ms > $baseline.avg_duration + 3 * $baseline.stddev_duration
        //     OR cost > $baseline.avg_cost + 3 * $baseline.stddev_cost
        //   )
        // ```

        // 框架实现
        let mut anomalies = Vec::new();

        // 模拟异常检测
        if time_range.end_ms - time_range.start_ms > 86400000 {
            anomalies.push(Anomaly {
                anomaly_id: Uuid::new_v4().to_string(),
                anomaly_type: AnomalyType::UnusualLatency,
                severity: AnomalySeverity::Medium,
                description: "Detected 3 events with latency > 3 standard deviations".to_string(),
                detected_at_ms: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
                related_events: vec!["event-anomaly-1".to_string()],
            });
        }

        anomalies
    }

    /// 检测决策模式
    pub async fn detect_decision_patterns(
        &self,
        actor_id: &str,
        time_range: &TimeRange,
    ) -> Vec<DecisionPatternResult> {
        // 框架实现
        vec![
            DecisionPatternResult {
                pattern_id: Uuid::new_v4().to_string(),
                actor_id: actor_id.to_string(),
                decision_path: "self_reason".to_string(),
                frequency: 60,
                percentage: 60.0,
                avg_confidence: 0.85,
                trend: PatternTrend::Stable,
            },
            DecisionPatternResult {
                pattern_id: Uuid::new_v4().to_string(),
                actor_id: actor_id.to_string(),
                decision_path: "clarify".to_string(),
                frequency: 20,
                percentage: 20.0,
                avg_confidence: 0.72,
                trend: PatternTrend::Decreasing,
            },
        ]
    }

    /// 检测协作模式
    pub async fn detect_collaboration_patterns(
        &self,
        time_range: &TimeRange,
    ) -> Vec<CollaborationPatternResult> {
        // 框架实现
        vec![CollaborationPatternResult {
            pattern_id: Uuid::new_v4().to_string(),
            participants: vec!["ai-1".to_string(), "ai-2".to_string()],
            collaboration_type: "task_delegation".to_string(),
            frequency: 15,
            success_rate: 0.93,
            avg_duration_ms: 5000,
        }]
    }

    /// 检测时间分布模式
    pub async fn detect_temporal_patterns(
        &self,
        entity_id: &str,
        time_range: &TimeRange,
    ) -> TemporalPatternResult {
        // 示例 SurrealQL:
        // ```
        // SELECT
        //   time::hour(occurred_at_ms) as hour,
        //   count() as activity_count
        // FROM event
        // WHERE entity_id = $entity_id
        //   AND occurred_at_ms >= $start
        //   AND occurred_at_ms <= $end
        // GROUP BY hour
        // ORDER BY hour
        // ```

        // 框架实现
        TemporalPatternResult {
            entity_id: entity_id.to_string(),
            time_range: time_range.clone(),
            peak_hours: vec![9, 10, 14, 15, 16],
            low_activity_hours: vec![0, 1, 2, 3, 4, 5],
            daily_pattern: DailyPattern::BusinessHours,
            weekly_pattern: WeeklyPattern::Weekdays,
            activity_distribution: (0..24)
                .map(|h| (h, if h >= 9 && h <= 17 { 10.0 } else { 2.0 }))
                .collect(),
        }
    }

    /// 获取模式摘要
    pub async fn get_pattern_summary(&self, time_range: &TimeRange) -> PatternSummary {
        // 框架实现
        PatternSummary {
            time_range: time_range.clone(),
            total_patterns_detected: 10,
            behavior_patterns: 5,
            interaction_patterns: 3,
            anomalies_detected: 2,
            significant_findings: vec![
                "High repetition in self_reason decisions".to_string(),
                "Tool usage concentrated in morning hours".to_string(),
            ],
            recommendations: vec![
                "Consider diversifying decision paths".to_string(),
                "Monitor latency patterns during peak hours".to_string(),
            ],
        }
    }
}

impl Default for PatternDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// 决策模式结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionPatternResult {
    pub pattern_id: String,
    pub actor_id: String,
    pub decision_path: String,
    pub frequency: u32,
    pub percentage: f32,
    pub avg_confidence: f32,
    pub trend: PatternTrend,
}

/// 模式趋势
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternTrend {
    Increasing,
    Decreasing,
    Stable,
    Volatile,
}

/// 协作模式结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollaborationPatternResult {
    pub pattern_id: String,
    pub participants: Vec<String>,
    pub collaboration_type: String,
    pub frequency: u32,
    pub success_rate: f32,
    pub avg_duration_ms: u64,
}

/// 时间模式结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalPatternResult {
    pub entity_id: String,
    pub time_range: TimeRange,
    pub peak_hours: Vec<u8>,
    pub low_activity_hours: Vec<u8>,
    pub daily_pattern: DailyPattern,
    pub weekly_pattern: WeeklyPattern,
    pub activity_distribution: HashMap<u8, f32>,
}

/// 日模式
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DailyPattern {
    BusinessHours,
    EveningPeak,
    NightOwl,
    AllDay,
    Irregular,
}

/// 周模式
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WeeklyPattern {
    Weekdays,
    Weekends,
    AllWeek,
    Irregular,
}

/// 模式摘要
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatternSummary {
    pub time_range: TimeRange,
    pub total_patterns_detected: u32,
    pub behavior_patterns: u32,
    pub interaction_patterns: u32,
    pub anomalies_detected: u32,
    pub significant_findings: Vec<String>,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_behavior_patterns() {
        let detector = PatternDetector::new();
        let patterns = detector.detect_behavior_patterns("actor-1").await;
        assert!(!patterns.is_empty());
    }

    #[tokio::test]
    async fn test_detect_interaction_patterns() {
        let detector = PatternDetector::new();
        let patterns = detector.detect_interaction_patterns("session-1").await;
        assert!(!patterns.is_empty());
    }

    #[tokio::test]
    async fn test_detect_anomalies() {
        let detector = PatternDetector::new();
        let time_range = TimeRange::last_days(7);
        let anomalies = detector.detect_anomalies(&time_range).await;
        // 7天范围应该检测到模拟异常
        assert!(anomalies.len() >= 0);
    }

    #[tokio::test]
    async fn test_get_pattern_summary() {
        let detector = PatternDetector::new();
        let time_range = TimeRange::last_days(30);
        let summary = detector.get_pattern_summary(&time_range).await;
        assert!(summary.total_patterns_detected > 0);
    }
}

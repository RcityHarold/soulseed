use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// SLO违规类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SloViolationType {
    /// Decide延迟超出阈值
    DecideLatencyExceeded,
    /// SyncPoint延迟超出阈值
    SyncPointLatencyExceeded,
    /// Late arrival比率超出阈值
    LateArrivalRateExceeded,
}

/// SLO违规记录
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SloViolation {
    pub violation_type: SloViolationType,
    pub threshold: f64,
    pub actual: f64,
    pub detected_at: OffsetDateTime,
    pub message: String,
}

impl SloViolation {
    pub fn new(
        violation_type: SloViolationType,
        threshold: f64,
        actual: f64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            violation_type,
            threshold,
            actual,
            detected_at: OffsetDateTime::now_utc(),
            message: message.into(),
        }
    }
}

/// SLO配置
#[derive(Clone, Debug)]
pub struct SloConfig {
    /// Decide操作p95延迟阈值（毫秒）
    pub decide_p95_threshold_ms: u64,
    /// SyncPoint操作p95延迟阈值（毫秒）
    pub sync_p95_threshold_ms: u64,
    /// Late arrival比率阈值（0.0-1.0）
    pub late_arrival_rate_threshold: f64,
    /// 保留最近N个样本用于计算百分位
    pub sample_window_size: usize,
}

impl Default for SloConfig {
    fn default() -> Self {
        Self {
            decide_p95_threshold_ms: 50,
            sync_p95_threshold_ms: 200,
            late_arrival_rate_threshold: 0.001, // 0.1%
            sample_window_size: 1000,
        }
    }
}

/// SLO监控器状态
#[derive(Default)]
struct SloMonitorState {
    /// Decide操作延迟样本（毫秒）
    decide_latencies: Vec<u64>,
    /// SyncPoint操作延迟样本（毫秒）
    sync_latencies: Vec<u64>,
    /// Late arrival计数
    late_arrival_count: u64,
    /// 总的SyncPoint计数
    total_sync_count: u64,
    /// 违规记录
    violations: Vec<SloViolation>,
}

impl SloMonitorState {
    /// 添加Decide延迟样本
    fn record_decide_latency(&mut self, latency_ms: u64, window_size: usize) {
        self.decide_latencies.push(latency_ms);
        if self.decide_latencies.len() > window_size {
            self.decide_latencies.remove(0);
        }
    }

    /// 添加SyncPoint延迟样本
    fn record_sync_latency(&mut self, latency_ms: u64, window_size: usize) {
        self.sync_latencies.push(latency_ms);
        if self.sync_latencies.len() > window_size {
            self.sync_latencies.remove(0);
        }
    }

    /// 记录late arrival
    fn record_late_arrival(&mut self) {
        self.late_arrival_count += 1;
        self.total_sync_count += 1;
    }

    /// 记录正常SyncPoint
    fn record_sync_point(&mut self) {
        self.total_sync_count += 1;
    }

    /// 计算百分位数
    fn calculate_percentile(samples: &[u64], percentile: f64) -> Option<u64> {
        if samples.is_empty() {
            return None;
        }

        let mut sorted = samples.to_vec();
        sorted.sort_unstable();

        let index = ((samples.len() as f64) * percentile) as usize;
        let index = index.min(sorted.len() - 1);
        Some(sorted[index])
    }

    /// 计算Decide p95延迟
    fn decide_p95(&self) -> Option<u64> {
        Self::calculate_percentile(&self.decide_latencies, 0.95)
    }

    /// 计算SyncPoint p95延迟
    fn sync_p95(&self) -> Option<u64> {
        Self::calculate_percentile(&self.sync_latencies, 0.95)
    }

    /// 计算late arrival比率
    fn late_arrival_rate(&self) -> f64 {
        if self.total_sync_count == 0 {
            return 0.0;
        }
        self.late_arrival_count as f64 / self.total_sync_count as f64
    }
}

/// SLO监控器
#[derive(Clone)]
pub struct SloMonitor {
    config: SloConfig,
    state: Arc<Mutex<SloMonitorState>>,
}

impl SloMonitor {
    pub fn new(config: SloConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(SloMonitorState::default())),
        }
    }

    /// 记录Decide操作延迟
    pub fn record_decide_latency(&self, latency_ms: u64) {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        state.record_decide_latency(latency_ms, self.config.sample_window_size);
    }

    /// 记录SyncPoint操作延迟
    pub fn record_sync_latency(&self, latency_ms: u64) {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        state.record_sync_latency(latency_ms, self.config.sample_window_size);
    }

    /// 记录late arrival
    pub fn record_late_arrival(&self) {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        state.record_late_arrival();
    }

    /// 记录正常SyncPoint
    pub fn record_sync_point(&self) {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        state.record_sync_point();
    }

    /// 获取Decide p95延迟
    pub fn decide_p95(&self) -> Option<u64> {
        let state = self.state.lock().expect("slo monitor poisoned");
        state.decide_p95()
    }

    /// 获取SyncPoint p95延迟
    pub fn sync_p95(&self) -> Option<u64> {
        let state = self.state.lock().expect("slo monitor poisoned");
        state.sync_p95()
    }

    /// 获取late arrival比率
    pub fn late_arrival_rate(&self) -> f64 {
        let state = self.state.lock().expect("slo monitor poisoned");
        state.late_arrival_rate()
    }

    /// 检查SLO违规
    pub fn check_slo_violations(&self) -> Vec<SloViolation> {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        let mut violations = Vec::new();

        // 检查Decide延迟
        if let Some(p95) = state.decide_p95() {
            if p95 > self.config.decide_p95_threshold_ms {
                let violation = SloViolation::new(
                    SloViolationType::DecideLatencyExceeded,
                    self.config.decide_p95_threshold_ms as f64,
                    p95 as f64,
                    format!(
                        "Decide p95 latency {}ms exceeds threshold {}ms",
                        p95, self.config.decide_p95_threshold_ms
                    ),
                );
                tracing::warn!(
                    "SLO violation detected: Decide p95={}ms > threshold={}ms",
                    p95,
                    self.config.decide_p95_threshold_ms
                );
                violations.push(violation.clone());
                state.violations.push(violation);
            }
        }

        // 检查SyncPoint延迟
        if let Some(p95) = state.sync_p95() {
            if p95 > self.config.sync_p95_threshold_ms {
                let violation = SloViolation::new(
                    SloViolationType::SyncPointLatencyExceeded,
                    self.config.sync_p95_threshold_ms as f64,
                    p95 as f64,
                    format!(
                        "SyncPoint p95 latency {}ms exceeds threshold {}ms",
                        p95, self.config.sync_p95_threshold_ms
                    ),
                );
                tracing::warn!(
                    "SLO violation detected: SyncPoint p95={}ms > threshold={}ms",
                    p95,
                    self.config.sync_p95_threshold_ms
                );
                violations.push(violation.clone());
                state.violations.push(violation);
            }
        }

        // 检查late arrival比率
        let late_rate = state.late_arrival_rate();
        if late_rate > self.config.late_arrival_rate_threshold {
            let violation = SloViolation::new(
                SloViolationType::LateArrivalRateExceeded,
                self.config.late_arrival_rate_threshold,
                late_rate,
                format!(
                    "Late arrival rate {:.3}% exceeds threshold {:.3}%",
                    late_rate * 100.0,
                    self.config.late_arrival_rate_threshold * 100.0
                ),
            );
            tracing::warn!(
                "SLO violation detected: Late arrival rate {:.3}% > threshold {:.3}%",
                late_rate * 100.0,
                self.config.late_arrival_rate_threshold * 100.0
            );
            violations.push(violation.clone());
            state.violations.push(violation);
        }

        violations
    }

    /// 获取所有历史违规记录
    pub fn violation_history(&self) -> Vec<SloViolation> {
        let state = self.state.lock().expect("slo monitor poisoned");
        state.violations.clone()
    }

    /// 获取统计快照
    pub fn snapshot(&self) -> SloSnapshot {
        let state = self.state.lock().expect("slo monitor poisoned");
        SloSnapshot {
            decide_p95: state.decide_p95(),
            sync_p95: state.sync_p95(),
            late_arrival_rate: state.late_arrival_rate(),
            late_arrival_count: state.late_arrival_count,
            total_sync_count: state.total_sync_count,
            decide_sample_count: state.decide_latencies.len(),
            sync_sample_count: state.sync_latencies.len(),
            violation_count: state.violations.len(),
        }
    }

    /// 重置所有统计数据（仅用于测试）
    pub fn reset(&self) {
        let mut state = self.state.lock().expect("slo monitor poisoned");
        *state = SloMonitorState::default();
    }
}

impl Default for SloMonitor {
    fn default() -> Self {
        Self::new(SloConfig::default())
    }
}

/// SLO统计快照
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SloSnapshot {
    pub decide_p95: Option<u64>,
    pub sync_p95: Option<u64>,
    pub late_arrival_rate: f64,
    pub late_arrival_count: u64,
    pub total_sync_count: u64,
    pub decide_sample_count: usize,
    pub sync_sample_count: usize,
    pub violation_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_calculation() {
        let samples = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

        // p50 应该在50-60之间
        let p50 = SloMonitorState::calculate_percentile(&samples, 0.50).unwrap();
        assert!(p50 >= 50 && p50 <= 60);

        // p95 应该在90-100之间
        let p95 = SloMonitorState::calculate_percentile(&samples, 0.95).unwrap();
        assert!(p95 >= 90 && p95 <= 100);

        // p99 应该接近100
        let p99 = SloMonitorState::calculate_percentile(&samples, 0.99).unwrap();
        assert_eq!(p99, 100);
    }

    #[test]
    fn test_decide_latency_monitoring() {
        let config = SloConfig {
            decide_p95_threshold_ms: 50,
            sample_window_size: 100,
            ..Default::default()
        };
        let monitor = SloMonitor::new(config);

        // 添加正常延迟样本
        for _ in 0..90 {
            monitor.record_decide_latency(30);
        }

        // 添加一些高延迟样本
        for _ in 0..10 {
            monitor.record_decide_latency(100);
        }

        let p95 = monitor.decide_p95().unwrap();
        assert!(p95 >= 90, "p95 should be around 100ms");

        // 检查违规
        let violations = monitor.check_slo_violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, SloViolationType::DecideLatencyExceeded);
    }

    #[test]
    fn test_sync_point_latency_monitoring() {
        let config = SloConfig {
            sync_p95_threshold_ms: 200,
            sample_window_size: 100,
            ..Default::default()
        };
        let monitor = SloMonitor::new(config);

        // 添加正常延迟样本
        for _ in 0..95 {
            monitor.record_sync_latency(150);
        }

        // 添加高延迟样本
        for _ in 0..5 {
            monitor.record_sync_latency(300);
        }

        let p95 = monitor.sync_p95().unwrap();
        assert!(p95 >= 250, "p95 should be around 300ms");

        // 检查违规
        let violations = monitor.check_slo_violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, SloViolationType::SyncPointLatencyExceeded);
    }

    #[test]
    fn test_late_arrival_rate_monitoring() {
        let config = SloConfig {
            late_arrival_rate_threshold: 0.01, // 1%
            ..Default::default()
        };
        let monitor = SloMonitor::new(config);

        // 记录990个正常SyncPoint
        for _ in 0..990 {
            monitor.record_sync_point();
        }

        // 记录10个late arrival
        for _ in 0..10 {
            monitor.record_late_arrival();
        }

        let rate = monitor.late_arrival_rate();
        assert!((rate - 0.01).abs() < 0.001, "Rate should be ~1%");

        // 添加更多late arrival超过阈值
        for _ in 0..5 {
            monitor.record_late_arrival();
        }

        let rate = monitor.late_arrival_rate();
        assert!(rate > 0.01, "Rate should exceed threshold");

        // 检查违规
        let violations = monitor.check_slo_violations();
        let late_violations: Vec<_> = violations
            .iter()
            .filter(|v| v.violation_type == SloViolationType::LateArrivalRateExceeded)
            .collect();
        assert_eq!(late_violations.len(), 1);
    }

    #[test]
    fn test_slo_snapshot() {
        let monitor = SloMonitor::default();

        monitor.record_decide_latency(40);
        monitor.record_decide_latency(50);
        monitor.record_sync_latency(180);
        monitor.record_sync_latency(190);
        monitor.record_sync_point();
        monitor.record_late_arrival();

        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.decide_sample_count, 2);
        assert_eq!(snapshot.sync_sample_count, 2);
        assert_eq!(snapshot.total_sync_count, 2);
        assert_eq!(snapshot.late_arrival_count, 1);
        assert!((snapshot.late_arrival_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_window_size_enforcement() {
        let config = SloConfig {
            sample_window_size: 5,
            ..Default::default()
        };
        let monitor = SloMonitor::new(config);

        // 添加超过窗口大小的样本
        for i in 0..10 {
            monitor.record_decide_latency((i + 1) * 10);
        }

        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.decide_sample_count, 5, "Should only keep last 5 samples");

        // p95应该基于最后5个样本 [60, 70, 80, 90, 100]
        let p95 = monitor.decide_p95().unwrap();
        assert!(p95 >= 90, "p95 should be based on recent samples");
    }

    #[test]
    fn test_no_violations_when_within_slo() {
        let monitor = SloMonitor::default();

        // 添加都在阈值内的样本
        for _ in 0..100 {
            monitor.record_decide_latency(40); // < 50ms
            monitor.record_sync_latency(180); // < 200ms
        }

        for _ in 0..1000 {
            monitor.record_sync_point(); // 0% late arrival
        }

        let violations = monitor.check_slo_violations();
        assert_eq!(violations.len(), 0, "Should have no violations");
    }
}

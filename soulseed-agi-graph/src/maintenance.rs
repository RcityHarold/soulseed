use serde::{Deserialize, Serialize};
use soulseed_agi_core_models::TenantId;
use std::collections::HashMap;
use std::time::Duration;

/// Interval/threshold configuration for图谱维护任务。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphMaintenanceConfig {
    pub redundant_edge_interval_ms: i64,
    pub redundant_edge_min_confidence: f32,
    pub redundant_edge_max_strength: f32,
    pub redundant_edge_max_age_ms: i64,
    pub community_detection_interval_ms: i64,
    pub community_detection_algorithm: String,
    pub community_detection_min_cluster: usize,
    pub density_monitor_interval_ms: i64,
    pub density_warn_threshold: f32,
    pub density_critical_threshold: f32,
}

impl Default for GraphMaintenanceConfig {
    fn default() -> Self {
        Self {
            redundant_edge_interval_ms: Duration::from_secs(15 * 60).as_millis() as i64,
            redundant_edge_min_confidence: 0.55,
            redundant_edge_max_strength: 0.2,
            redundant_edge_max_age_ms: Duration::from_secs(24 * 3600).as_millis() as i64,
            community_detection_interval_ms: Duration::from_secs(4 * 3600).as_millis() as i64,
            community_detection_algorithm: "louvain".into(),
            community_detection_min_cluster: 8,
            density_monitor_interval_ms: Duration::from_secs(10 * 60).as_millis() as i64,
            density_warn_threshold: 0.42,
            density_critical_threshold: 0.68,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceTaskKind {
    RedundantEdgeCleanup,
    CommunityDetection,
    DensityMonitoring,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaintenanceParameters {
    RedundantEdge {
        min_confidence: f32,
        max_strength: f32,
        max_age_ms: i64,
    },
    CommunityDetection {
        algorithm: String,
        min_cluster: usize,
    },
    DensityMonitoring {
        warn_threshold: f32,
        critical_threshold: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaintenanceJob {
    pub task: MaintenanceTaskKind,
    pub scheduled_at_ms: i64,
    pub parameters: MaintenanceParameters,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GraphMaintenanceState {
    last_run_ms: HashMap<MaintenanceTaskKind, i64>,
}

impl GraphMaintenanceState {
    pub fn last_run(&self, kind: MaintenanceTaskKind) -> Option<i64> {
        self.last_run_ms.get(&kind).copied()
    }

    pub fn record_run(&mut self, kind: MaintenanceTaskKind, timestamp_ms: i64) {
        self.last_run_ms.insert(kind, timestamp_ms);
    }
}

#[derive(Clone, Debug)]
pub struct MaintenancePlanner {
    cfg: GraphMaintenanceConfig,
}

impl MaintenancePlanner {
    pub fn new(cfg: GraphMaintenanceConfig) -> Self {
        Self { cfg }
    }

    pub fn plan(&self, state: &GraphMaintenanceState, now_ms: i64) -> Vec<MaintenanceJob> {
        vec![
            self.schedule_redundant_edges(state, now_ms),
            self.schedule_community_detection(state, now_ms),
            self.schedule_density_monitor(state, now_ms),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn schedule_redundant_edges(
        &self,
        state: &GraphMaintenanceState,
        now_ms: i64,
    ) -> Option<MaintenanceJob> {
        let due = is_due(
            state.last_run(MaintenanceTaskKind::RedundantEdgeCleanup),
            now_ms,
            self.cfg.redundant_edge_interval_ms,
        );
        if !due {
            return None;
        }
        Some(MaintenanceJob {
            task: MaintenanceTaskKind::RedundantEdgeCleanup,
            scheduled_at_ms: now_ms,
            parameters: MaintenanceParameters::RedundantEdge {
                min_confidence: self.cfg.redundant_edge_min_confidence,
                max_strength: self.cfg.redundant_edge_max_strength,
                max_age_ms: self.cfg.redundant_edge_max_age_ms,
            },
        })
    }

    fn schedule_community_detection(
        &self,
        state: &GraphMaintenanceState,
        now_ms: i64,
    ) -> Option<MaintenanceJob> {
        let due = is_due(
            state.last_run(MaintenanceTaskKind::CommunityDetection),
            now_ms,
            self.cfg.community_detection_interval_ms,
        );
        if !due {
            return None;
        }
        Some(MaintenanceJob {
            task: MaintenanceTaskKind::CommunityDetection,
            scheduled_at_ms: now_ms,
            parameters: MaintenanceParameters::CommunityDetection {
                algorithm: self.cfg.community_detection_algorithm.clone(),
                min_cluster: self.cfg.community_detection_min_cluster,
            },
        })
    }

    fn schedule_density_monitor(
        &self,
        state: &GraphMaintenanceState,
        now_ms: i64,
    ) -> Option<MaintenanceJob> {
        let due = is_due(
            state.last_run(MaintenanceTaskKind::DensityMonitoring),
            now_ms,
            self.cfg.density_monitor_interval_ms,
        );
        if !due {
            return None;
        }
        Some(MaintenanceJob {
            task: MaintenanceTaskKind::DensityMonitoring,
            scheduled_at_ms: now_ms,
            parameters: MaintenanceParameters::DensityMonitoring {
                warn_threshold: self.cfg.density_warn_threshold,
                critical_threshold: self.cfg.density_critical_threshold,
            },
        })
    }
}

fn is_due(last_run: Option<i64>, now_ms: i64, interval_ms: i64) -> bool {
    let Some(last) = last_run else {
        return true;
    };
    now_ms.saturating_sub(last) >= interval_ms
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaintenanceMetricsReport {
    pub tenant_id: Option<TenantId>,
    pub task: MaintenanceTaskKind,
    pub started_ms: i64,
    pub completed_ms: i64,
    pub processed_edges: u64,
    pub processed_nodes: u64,
    pub dropped_edges: u64,
    pub clusters_detected: Option<u32>,
    pub density_ratio: Option<f32>,
    pub degradation_reason: Option<String>,
}

impl Default for MaintenanceMetricsReport {
    fn default() -> Self {
        Self {
            tenant_id: None,
            task: MaintenanceTaskKind::DensityMonitoring,
            started_ms: 0,
            completed_ms: 0,
            processed_edges: 0,
            processed_nodes: 0,
            dropped_edges: 0,
            clusters_detected: None,
            density_ratio: None,
            degradation_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::TenantId;

    fn cfg() -> GraphMaintenanceConfig {
        GraphMaintenanceConfig {
            redundant_edge_interval_ms: 1_000,
            redundant_edge_min_confidence: 0.6,
            redundant_edge_max_strength: 0.3,
            redundant_edge_max_age_ms: 10_000,
            community_detection_interval_ms: 5_000,
            community_detection_algorithm: "louvain".into(),
            community_detection_min_cluster: 5,
            density_monitor_interval_ms: 2_000,
            density_warn_threshold: 0.4,
            density_critical_threshold: 0.7,
        }
    }

    #[test]
    fn planner_emits_all_due_jobs_when_intervals_expire() {
        let planner = MaintenancePlanner::new(cfg());
        let mut state = GraphMaintenanceState::default();
        state.record_run(MaintenanceTaskKind::RedundantEdgeCleanup, 0);
        state.record_run(MaintenanceTaskKind::CommunityDetection, 0);
        state.record_run(MaintenanceTaskKind::DensityMonitoring, 0);

        let jobs = planner.plan(&state, 6_000);
        assert_eq!(jobs.len(), 3);
        assert!(jobs
            .iter()
            .any(|job| job.task == MaintenanceTaskKind::RedundantEdgeCleanup));
        assert!(jobs
            .iter()
            .any(|job| job.task == MaintenanceTaskKind::CommunityDetection));
        assert!(jobs
            .iter()
            .any(|job| job.task == MaintenanceTaskKind::DensityMonitoring));
    }

    #[test]
    fn planner_skips_jobs_if_interval_not_met() {
        let planner = MaintenancePlanner::new(cfg());
        let mut state = GraphMaintenanceState::default();
        state.record_run(MaintenanceTaskKind::RedundantEdgeCleanup, 4_400);
        state.record_run(MaintenanceTaskKind::CommunityDetection, 4_900);
        state.record_run(MaintenanceTaskKind::DensityMonitoring, 2_900);

        let jobs = planner.plan(&state, 5_000);
        assert!(jobs
            .iter()
            .all(|job| job.task != MaintenanceTaskKind::RedundantEdgeCleanup));
        assert!(jobs
            .iter()
            .all(|job| job.task != MaintenanceTaskKind::CommunityDetection));
        assert!(jobs
            .iter()
            .any(|job| job.task == MaintenanceTaskKind::DensityMonitoring));
    }

    #[test]
    fn metrics_report_captures_core_fields() {
        let report = MaintenanceMetricsReport {
            tenant_id: Some(TenantId::new(1)),
            task: MaintenanceTaskKind::DensityMonitoring,
            started_ms: 100,
            completed_ms: 200,
            processed_edges: 32,
            processed_nodes: 8,
            dropped_edges: 2,
            clusters_detected: None,
            density_ratio: Some(0.52),
            degradation_reason: Some("density_high".into()),
        };
        assert_eq!(report.completed_ms - report.started_ms, 100);
        assert_eq!(report.density_ratio, Some(0.52));
    }
}

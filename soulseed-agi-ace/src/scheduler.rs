use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{BudgetSnapshot, CycleLane, CycleSchedule, ScheduleOutcome, new_cycle_id};
use serde_json::Value;
use soulseed_agi_core_models::awareness::AwarenessAnchor;
use soulseed_agi_tools::dto::ToolPlan;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct SchedulerConfig {
    pub max_pending_per_tenant: usize,
    pub allow_parallel_lanes: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_pending_per_tenant: 8,
            allow_parallel_lanes: false,
        }
    }
}

#[derive(Default)]
struct SchedulerState {
    pending: HashMap<u64, VecDeque<CycleSchedule>>,
    active: HashMap<u64, CycleLane>,
}

#[derive(Clone)]
pub struct CycleScheduler {
    cfg: SchedulerConfig,
    state: Arc<Mutex<SchedulerState>>,
}

impl CycleScheduler {
    pub fn new(cfg: SchedulerConfig) -> Self {
        Self {
            cfg,
            state: Arc::new(Mutex::new(SchedulerState::default())),
        }
    }

    pub fn schedule(
        &self,
        anchor: AwarenessAnchor,
        lane: CycleLane,
        tool_plan: Option<ToolPlan>,
        llm_plan: Option<Value>,
        budget: BudgetSnapshot,
    ) -> Result<ScheduleOutcome, AceError> {
        let tenant = anchor.tenant_id.into_inner();
        if !self.cfg.allow_parallel_lanes {
            let guard = self.state.lock().unwrap();
            if let Some(active_lane) = guard.active.get(&tenant) {
                if matches!(
                    (active_lane, &lane),
                    (CycleLane::Clarify, CycleLane::Clarify)
                ) {
                    return Ok(ScheduleOutcome {
                        accepted: false,
                        reason: Some("clarify_lane_busy".into()),
                        cycle: None,
                    });
                }
            }
        }

        let mut guard = self.state.lock().unwrap();
        let entry = guard.pending.entry(tenant).or_default();
        if !self.cfg.allow_parallel_lanes && matches!(lane, CycleLane::Clarify) {
            if entry.iter().any(|c| matches!(c.lane, CycleLane::Clarify)) {
                return Ok(ScheduleOutcome {
                    accepted: false,
                    reason: Some("clarify_lane_busy".into()),
                    cycle: None,
                });
            }
        }

        if entry.len() >= self.cfg.max_pending_per_tenant {
            return Ok(ScheduleOutcome {
                accepted: false,
                reason: Some("pending_limit".into()),
                cycle: None,
            });
        }

        let cycle = CycleSchedule {
            cycle_id: new_cycle_id(),
            lane,
            anchor,
            tool_plan,
            llm_plan,
            budget,
            created_at: OffsetDateTime::now_utc(),
        };
        entry.push_back(cycle.clone());
        Ok(ScheduleOutcome {
            accepted: true,
            reason: None,
            cycle: Some(cycle),
        })
    }

    pub fn start_next(&self, tenant: u64) -> Option<CycleSchedule> {
        let mut guard = self.state.lock().unwrap();
        let queue = guard.pending.get_mut(&tenant)?;
        let cycle = queue.pop_front()?;
        guard.active.insert(tenant, cycle.lane.clone());
        Some(cycle)
    }

    pub fn finish(&self, tenant: u64) {
        let mut guard = self.state.lock().unwrap();
        guard.active.remove(&tenant);
    }
}

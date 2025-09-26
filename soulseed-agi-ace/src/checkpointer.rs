use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::errors::AceError;
use crate::types::{CheckpointState, CycleLane};
use time::OffsetDateTime;

#[derive(Clone, Default)]
pub struct Checkpointer {
    inner: Arc<Mutex<HashMap<(u64, u64), CheckpointState>>>,
}

impl Checkpointer {
    pub fn record(&self, state: CheckpointState) {
        let key = (state.tenant_id.into_inner(), state.cycle_id.0);
        self.inner.lock().unwrap().insert(key, state);
    }

    pub fn fetch(&self, tenant: u64, cycle: u64) -> Option<CheckpointState> {
        self.inner.lock().unwrap().get(&(tenant, cycle)).cloned()
    }

    pub fn ensure_lane_idle(
        &self,
        tenant: u64,
        lane: &CycleLane,
        window: time::Duration,
    ) -> Result<(), AceError> {
        if let Some(state) = self
            .inner
            .lock()
            .unwrap()
            .values()
            .find(|st| st.tenant_id.into_inner() == tenant && &st.lane == lane)
        {
            let elapsed = OffsetDateTime::now_utc() - state.since;
            if elapsed < window {
                return Err(AceError::ScheduleConflict(format!(
                    "lane {:?} busy for tenant {}",
                    lane, tenant
                )));
            }
        }
        Ok(())
    }

    pub fn finish(&self, tenant: u64) {
        self.inner.lock().unwrap().retain(|(t, _), _| *t != tenant);
    }
}

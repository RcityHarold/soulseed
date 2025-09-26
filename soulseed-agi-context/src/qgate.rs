use std::sync::Mutex;

use crate::{
    errors::QualityFailure,
    traits::QualityGate,
    types::{Level, SummaryUnit},
};

#[derive(Debug)]
pub struct QualityGateMock {
    pub fail_level: Option<Level>,
    pub fail_once: bool,
    flag: Mutex<bool>,
}

impl Default for QualityGateMock {
    fn default() -> Self {
        Self {
            fail_level: None,
            fail_once: false,
            flag: Mutex::new(false),
        }
    }
}

impl QualityGateMock {
    pub fn new(fail_level: Option<Level>, fail_once: bool) -> Self {
        Self {
            fail_level,
            fail_once,
            flag: Mutex::new(false),
        }
    }
}

impl QualityGate for QualityGateMock {
    fn evaluate(&self, summary: &SummaryUnit) -> Result<(), QualityFailure> {
        if let Some(level) = self.fail_level {
            if summary.level == level {
                let mut flag = self.flag.lock().unwrap();
                if self.fail_once && *flag {
                    return Ok(());
                }
                *flag = true;
                return Err(QualityFailure {
                    su_id: summary.su_id.clone(),
                    level,
                    reason: "quality_gate_fail".into(),
                });
            }
        }
        Ok(())
    }
}

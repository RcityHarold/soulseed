use std::sync::Mutex;

use crate::{
    config::ContextConfig,
    errors::QualityFailure,
    traits::QualityGate,
    types::{Level, SummaryUnit},
};

#[derive(Debug)]
pub struct QualityGateStrict {
    pub fail_level: Option<Level>,
    pub fail_once: bool,
    flag: Mutex<bool>,
}

impl Default for QualityGateStrict {
    fn default() -> Self {
        Self {
            fail_level: None,
            fail_once: false,
            flag: Mutex::new(false),
        }
    }
}

impl QualityGateStrict {
    pub fn new(fail_level: Option<Level>, fail_once: bool) -> Self {
        Self {
            fail_level,
            fail_once,
            flag: Mutex::new(false),
        }
    }
}

impl QualityGate for QualityGateStrict {
    fn evaluate(&self, summary: &SummaryUnit, cfg: &ContextConfig) -> Result<(), QualityFailure> {
        if summary.evidence.is_none() {
            return Err(QualityFailure {
                su_id: summary.su_id.clone(),
                level: summary.level,
                reason: "pointer_missing".into(),
            });
        }
        if !summary.quality.pointer_ok {
            return Err(QualityFailure {
                su_id: summary.su_id.clone(),
                level: summary.level,
                reason: "pointer_invalid".into(),
            });
        }

        let thresholds = &cfg.quality_thresholds;
        if summary.quality.qag + f32::EPSILON < thresholds.qag {
            return Err(QualityFailure {
                su_id: summary.su_id.clone(),
                level: summary.level,
                reason: format!(
                    "qag_below_threshold:{:.2}<{:.2}",
                    summary.quality.qag, thresholds.qag
                ),
            });
        }
        if summary.quality.coverage + f32::EPSILON < thresholds.coverage {
            return Err(QualityFailure {
                su_id: summary.su_id.clone(),
                level: summary.level,
                reason: format!(
                    "coverage_below_threshold:{:.2}<{:.2}",
                    summary.quality.coverage, thresholds.coverage
                ),
            });
        }
        if summary.quality.nli_contrad - f32::EPSILON > thresholds.nli_contrad {
            return Err(QualityFailure {
                su_id: summary.su_id.clone(),
                level: summary.level,
                reason: format!(
                    "nli_above_threshold:{:.2}>{:.2}",
                    summary.quality.nli_contrad, thresholds.nli_contrad
                ),
            });
        }

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

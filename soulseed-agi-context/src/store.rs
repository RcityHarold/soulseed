use std::sync::Mutex;

use crate::traits::ContextStore;
use crate::types::{CompressionPlan, CompressionReport, SummaryUnit};

#[derive(Default)]
pub struct InMemoryStore {
    plans: Mutex<Vec<CompressionPlan>>,
    reports: Mutex<Vec<CompressionReport>>,
    summaries: Mutex<Vec<SummaryUnit>>,
}

impl ContextStore for InMemoryStore {
    fn record_plan(&self, plan: &CompressionPlan) {
        self.plans.lock().unwrap().push(plan.clone());
    }

    fn record_report(&self, report: &CompressionReport) {
        self.reports.lock().unwrap().push(report.clone());
    }

    fn record_summary(&self, summary: &SummaryUnit) {
        self.summaries.lock().unwrap().push(summary.clone());
    }
}

use std::sync::Mutex;

use crate::traits::ContextStore;
use crate::types::{
    CompressionPlan, CompressionReport, ContextEvent, RedactionReport, SummaryUnit,
};

#[derive(Default)]
pub struct InMemoryStore {
    plans: Mutex<Vec<CompressionPlan>>,
    reports: Mutex<Vec<CompressionReport>>,
    summaries: Mutex<Vec<SummaryUnit>>,
    redactions: Mutex<Vec<RedactionReport>>,
    events: Mutex<Vec<ContextEvent>>,
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

    fn record_redaction(&self, report: &RedactionReport) {
        self.redactions.lock().unwrap().push(report.clone());
    }

    fn record_event(&self, event: &ContextEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

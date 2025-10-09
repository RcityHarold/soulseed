pub trait AceMetrics: Send + Sync {
    fn counter(&self, name: &str, value: f64, labels: &[(&str, String)]);
    fn gauge(&self, name: &str, value: f64, labels: &[(&str, String)]);
}

#[derive(Clone, Copy, Default)]
pub struct NoopMetrics;

impl AceMetrics for NoopMetrics {
    fn counter(&self, _name: &str, _value: f64, _labels: &[(&str, String)]) {}

    fn gauge(&self, _name: &str, _value: f64, _labels: &[(&str, String)]) {}
}

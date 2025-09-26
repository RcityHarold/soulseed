pub trait Observability: Send + Sync {
    fn emit_metric(&self, name: &str, value: f64, tags: &[(&str, String)]);
}

pub struct NoopMetrics;

impl Observability for NoopMetrics {
    fn emit_metric(&self, _name: &str, _value: f64, _tags: &[(&str, String)]) {}
}

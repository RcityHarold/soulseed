pub struct NoopObs;

impl crate::traits::Observability for NoopObs {
    fn emit_metric(&self, _name: &str, _value: f64, _tags: &[(&str, String)]) {}
}

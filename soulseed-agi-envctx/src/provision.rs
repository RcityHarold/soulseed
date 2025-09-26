use crate::dto::{Anchor, EnvironmentContext};
use crate::engine::EnvironmentEngine;
use crate::errors::Result;
use crate::facade::{DegradationReporter, EnvironmentDataProvider};

pub struct EnvCtxProvisioner<P, R> {
    engine: EnvironmentEngine<P, R>,
}

impl<P, R> EnvCtxProvisioner<P, R>
where
    P: EnvironmentDataProvider,
    R: DegradationReporter,
{
    pub fn new(provider: P, reporter: R) -> Self {
        Self {
            engine: EnvironmentEngine::new(provider, reporter),
        }
    }

    pub fn assemble(&self, anchor: Anchor) -> Result<EnvironmentContext> {
        self.engine.assemble(anchor)
    }
}

impl<P, R> From<EnvironmentEngine<P, R>> for EnvCtxProvisioner<P, R>
where
    P: EnvironmentDataProvider,
    R: DegradationReporter,
{
    fn from(engine: EnvironmentEngine<P, R>) -> Self {
        Self { engine }
    }
}

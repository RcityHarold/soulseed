use crate::dto::{
    Anchor, ExternalSystems, InteractionObject, InternalScene, LifeJourney, SourceVersions,
    ToolPermission,
};
use crate::errors::Result;

/// Provides the raw environment signals that will eventually come from Soulbase.
/// The production implementation should call Soulbase thin-waist endpoints.
pub trait EnvironmentDataProvider: Send + Sync {
    fn load_internal_scene(&self, anchor: &Anchor) -> Result<InternalScene>;
    fn load_external_systems(&self, anchor: &Anchor) -> Result<ExternalSystems>;
    fn load_interaction_object(&self, anchor: &Anchor) -> Result<InteractionObject>;
    fn load_tool_permission(&self, anchor: &Anchor) -> Result<ToolPermission>;
    fn load_life_journey(&self, anchor: &Anchor) -> Result<LifeJourney>;
    fn load_source_versions(&self, anchor: &Anchor) -> Result<SourceVersions>;
}

/// Allows the engine to surface contextual degradation that callers can relay to Explain.
pub trait DegradationReporter: Send + Sync {
    fn report_degradation(&self, anchor: &Anchor, code: &str, detail: Option<&str>);
}

/// Default no-op reporter, usable before Soulbase observability hook is wired.
pub struct NoopReporter;

impl DegradationReporter for NoopReporter {
    fn report_degradation(&self, _anchor: &Anchor, _code: &str, _detail: Option<&str>) {}
}

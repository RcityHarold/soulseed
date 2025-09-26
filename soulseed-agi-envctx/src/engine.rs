use time::OffsetDateTime;

use crate::canon::compute_digest;
use crate::dto::{
    AIView, Anchor, ConversationSummary, DegradationReason, EnvironmentContext, ExternalSystems,
    FreshnessState, GroupView, InteractionObject, InternalScene, LatencyWindow, LifeJourney,
    LifeMilestone, NetworkQuality, ServiceFreshness, SoulState, SourceVersions, TaskSummary,
    ToolDefLite, ToolPermission, VersionPointer,
};
use crate::errors::{EnvCtxError, Result};
use crate::facade::{DegradationReporter, EnvironmentDataProvider};

pub struct EnvironmentEngine<P, R> {
    provider: P,
    reporter: R,
}

impl<P, R> EnvironmentEngine<P, R>
where
    P: EnvironmentDataProvider,
    R: DegradationReporter,
{
    pub fn new(provider: P, reporter: R) -> Self {
        Self { provider, reporter }
    }

    pub fn assemble(&self, anchor: Anchor) -> Result<EnvironmentContext> {
        self.validate_anchor(&anchor)?;

        let mut degradation: Option<DegradationReason> = None;

        let internal_scene = self.take_or_fallback(
            self.provider.load_internal_scene(&anchor),
            &anchor,
            &mut degradation,
            "envctx.internal_scene",
            fallback_internal_scene,
        )?;

        let external_systems = self.take_or_fallback(
            self.provider.load_external_systems(&anchor),
            &anchor,
            &mut degradation,
            "envctx.external_systems",
            fallback_external_systems,
        )?;

        let interaction_object = self.take_or_fallback(
            self.provider.load_interaction_object(&anchor),
            &anchor,
            &mut degradation,
            "envctx.interaction_object",
            fallback_interaction_object,
        )?;

        let tool_permission = self.take_or_fallback(
            self.provider.load_tool_permission(&anchor),
            &anchor,
            &mut degradation,
            "envctx.tool_permission",
            fallback_tool_permission,
        )?;

        let life_journey = self.take_or_fallback(
            self.provider.load_life_journey(&anchor),
            &anchor,
            &mut degradation,
            "envctx.life_journey",
            fallback_life_journey,
        )?;

        let source_versions = self.take_or_fallback(
            self.provider.load_source_versions(&anchor),
            &anchor,
            &mut degradation,
            "envctx.source_versions",
            fallback_source_versions,
        )?;

        let mut context = EnvironmentContext {
            anchor,
            internal_scene,
            external_systems,
            interaction_object,
            tool_permission,
            life_journey,
            source_versions,
            context_digest: String::new(),
            degradation_reason: degradation.clone(),
        };

        context.context_digest = compute_digest(&context)?;
        Ok(context)
    }

    fn validate_anchor(&self, anchor: &Anchor) -> Result<()> {
        if anchor.config_snapshot_hash.is_empty() {
            return Err(EnvCtxError::Missing("config_snapshot_hash"));
        }
        if anchor.schema_v == 0 {
            return Err(EnvCtxError::Missing("schema_v"));
        }
        if matches!(
            anchor.access_class,
            soulseed_agi_core_models::AccessClass::Restricted
        ) && anchor.provenance.is_none()
        {
            return Err(EnvCtxError::Privacy(
                "provenance required for restricted anchor",
            ));
        }
        Ok(())
    }

    fn take_or_fallback<T>(
        &self,
        value: Result<T>,
        anchor: &Anchor,
        degradation: &mut Option<DegradationReason>,
        code: &str,
        fallback: fn() -> T,
    ) -> Result<T> {
        match value {
            Ok(v) => Ok(v),
            Err(EnvCtxError::SourceUnavailable { endpoint, reason }) => {
                let detail = format!("endpoint={endpoint}; reason={reason}");
                self.reporter
                    .report_degradation(anchor, code, Some(detail.as_str()));
                if degradation.is_none() {
                    *degradation = Some(DegradationReason::new(code, Some(detail.clone())));
                }
                Ok(fallback())
            }
            Err(EnvCtxError::Missing(name)) => {
                self.reporter.report_degradation(anchor, code, Some(name));
                if degradation.is_none() {
                    *degradation = Some(DegradationReason::new(code, Some(name.to_string())));
                }
                Ok(fallback())
            }
            Err(err) => Err(err),
        }
    }
}

fn fallback_internal_scene() -> InternalScene {
    InternalScene {
        conversation: ConversationSummary {
            rounds: 0,
            topics: Vec::new(),
            scene: "unknown".into(),
        },
        task: TaskSummary {
            goal: "undetermined".into(),
            constraints: Vec::new(),
        },
        latency_window: LatencyWindow {
            p50_ms: 0,
            p95_ms: 0,
        },
        risk_flag: crate::dto::RiskLevel::Medium,
    }
}

fn fallback_external_systems() -> ExternalSystems {
    ExternalSystems {
        environment: "unknown".into(),
        region: "unknown".into(),
        timezone: "UTC".into(),
        locale: "en-US".into(),
        network_quality: NetworkQuality::Fair,
        service_freshness: vec![ServiceFreshness {
            name: "graph".into(),
            freshness: FreshnessState::Unknown,
            last_synced_at: None,
        }],
        policy_digest: "unknown".into(),
    }
}

fn fallback_interaction_object() -> InteractionObject {
    InteractionObject {
        human: None,
        ai_companions: vec![AIView {
            id: soulseed_agi_core_models::AIId::new(0),
            soul_state: SoulState::Reflecting,
        }],
        group: Some(GroupView {
            members: 0,
            roles: Vec::new(),
        }),
    }
}

fn fallback_tool_permission() -> ToolPermission {
    ToolPermission {
        available_tools: vec![ToolDefLite {
            tool_id: "noop".into(),
            version: "0".into(),
            capabilities: Vec::new(),
            risk_level: Some("unknown".into()),
            scope: Some("fallback".into()),
        }],
        policy_digest: "unknown".into(),
    }
}

fn fallback_life_journey() -> LifeJourney {
    LifeJourney {
        milestones: vec![LifeMilestone {
            name: "bootstrap".into(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            significance: None,
        }],
        current_arc: Some("initializing".into()),
    }
}

fn fallback_source_versions() -> SourceVersions {
    SourceVersions {
        graph_snapshot: VersionPointer {
            digest: "unknown".into(),
            at: None,
        },
        policy_snapshot: VersionPointer {
            digest: "unknown".into(),
            at: None,
        },
        observe_watermark: None,
    }
}

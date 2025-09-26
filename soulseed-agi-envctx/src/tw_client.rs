use crate::dto::{
    Anchor, ExternalSystems, InteractionObject, InternalScene, LifeJourney, SourceVersions,
    ToolPermission,
};
use crate::errors::{EnvCtxError, Result};
use crate::facade::EnvironmentDataProvider;

/// Soulbase Thin-Waist 返回错误
#[derive(Debug, thiserror::Error)]
pub enum ThinWaistError {
    #[error("soulbase thin-waist failure on {endpoint}: {message}")]
    Failure {
        endpoint: &'static str,
        message: String,
    },
}

/// Soulbase Thin-Waist 客户端行为约束。
///
/// 真实环境中应由调用 REST/gRPC Gateway 的适配器实现该 trait。
pub trait ThinWaistClient: Send + Sync {
    fn internal_scene(&self, anchor: &Anchor)
        -> std::result::Result<InternalScene, ThinWaistError>;
    fn external_systems(
        &self,
        anchor: &Anchor,
    ) -> std::result::Result<ExternalSystems, ThinWaistError>;
    fn interaction_object(
        &self,
        anchor: &Anchor,
    ) -> std::result::Result<InteractionObject, ThinWaistError>;
    fn tool_permission(
        &self,
        anchor: &Anchor,
    ) -> std::result::Result<ToolPermission, ThinWaistError>;
    fn life_journey(&self, anchor: &Anchor) -> std::result::Result<LifeJourney, ThinWaistError>;
    fn source_versions(
        &self,
        anchor: &Anchor,
    ) -> std::result::Result<SourceVersions, ThinWaistError>;
}

/// 使用 Soulbase Thin-Waist 客户端实现 `EnvironmentDataProvider`。
#[derive(Clone)]
pub struct SoulbaseEnvProvider<C> {
    client: C,
}

impl<C> SoulbaseEnvProvider<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    fn map_err(endpoint: &'static str, err: ThinWaistError) -> EnvCtxError {
        match err {
            ThinWaistError::Failure { message, .. } => EnvCtxError::SourceUnavailable {
                endpoint,
                reason: message,
            },
        }
    }
}

impl<C> EnvironmentDataProvider for SoulbaseEnvProvider<C>
where
    C: ThinWaistClient,
{
    fn load_internal_scene(&self, anchor: &Anchor) -> Result<InternalScene> {
        self.client
            .internal_scene(anchor)
            .map_err(|err| Self::map_err("envctx.internal_scene", err))
    }

    fn load_external_systems(&self, anchor: &Anchor) -> Result<ExternalSystems> {
        self.client
            .external_systems(anchor)
            .map_err(|err| Self::map_err("envctx.external_systems", err))
    }

    fn load_interaction_object(&self, anchor: &Anchor) -> Result<InteractionObject> {
        self.client
            .interaction_object(anchor)
            .map_err(|err| Self::map_err("envctx.interaction_object", err))
    }

    fn load_tool_permission(&self, anchor: &Anchor) -> Result<ToolPermission> {
        self.client
            .tool_permission(anchor)
            .map_err(|err| Self::map_err("envctx.tool_permission", err))
    }

    fn load_life_journey(&self, anchor: &Anchor) -> Result<LifeJourney> {
        self.client
            .life_journey(anchor)
            .map_err(|err| Self::map_err("envctx.life_journey", err))
    }

    fn load_source_versions(&self, anchor: &Anchor) -> Result<SourceVersions> {
        self.client
            .source_versions(anchor)
            .map_err(|err| Self::map_err("envctx.source_versions", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{ConversationSummary, LatencyWindow, RiskLevel, TaskSummary, VersionPointer};

    #[derive(Clone)]
    struct OkTwClient;

    impl ThinWaistClient for OkTwClient {
        fn internal_scene(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<InternalScene, ThinWaistError> {
            Ok(InternalScene {
                conversation: ConversationSummary {
                    rounds: 1,
                    topics: vec![],
                    scene: "demo".into(),
                },
                task: TaskSummary {
                    goal: "goal".into(),
                    constraints: vec![],
                },
                latency_window: LatencyWindow {
                    p50_ms: 80,
                    p95_ms: 200,
                },
                risk_flag: RiskLevel::Low,
            })
        }

        fn external_systems(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<ExternalSystems, ThinWaistError> {
            Ok(ExternalSystems {
                environment: "dev".into(),
                region: "moon".into(),
                timezone: "UTC".into(),
                locale: "en-US".into(),
                network_quality: crate::dto::NetworkQuality::Good,
                service_freshness: vec![],
                policy_digest: "pol".into(),
            })
        }

        fn interaction_object(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<InteractionObject, ThinWaistError> {
            Ok(InteractionObject {
                human: None,
                ai_companions: Vec::new(),
                group: None,
            })
        }

        fn tool_permission(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<ToolPermission, ThinWaistError> {
            Ok(ToolPermission {
                available_tools: Vec::new(),
                policy_digest: "tools".into(),
            })
        }

        fn life_journey(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<LifeJourney, ThinWaistError> {
            Ok(LifeJourney {
                milestones: Vec::new(),
                current_arc: None,
            })
        }

        fn source_versions(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<SourceVersions, ThinWaistError> {
            Ok(SourceVersions {
                graph_snapshot: VersionPointer {
                    digest: "g".into(),
                    at: None,
                },
                policy_snapshot: VersionPointer {
                    digest: "p".into(),
                    at: None,
                },
                observe_watermark: None,
            })
        }
    }

    #[derive(Clone)]
    struct FailingTwClient;

    impl ThinWaistClient for FailingTwClient {
        fn internal_scene(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<InternalScene, ThinWaistError> {
            Err(ThinWaistError::Failure {
                endpoint: "envctx.internal_scene",
                message: "timeout".into(),
            })
        }
        fn external_systems(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<ExternalSystems, ThinWaistError> {
            unreachable!()
        }
        fn interaction_object(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<InteractionObject, ThinWaistError> {
            unreachable!()
        }
        fn tool_permission(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<ToolPermission, ThinWaistError> {
            unreachable!()
        }
        fn life_journey(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<LifeJourney, ThinWaistError> {
            unreachable!()
        }
        fn source_versions(
            &self,
            _anchor: &Anchor,
        ) -> std::result::Result<SourceVersions, ThinWaistError> {
            unreachable!()
        }
    }

    fn anchor() -> Anchor {
        Anchor {
            tenant_id: soulseed_agi_core_models::TenantId::new(1),
            envelope_id: uuid::Uuid::now_v7(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: None,
            sequence_number: None,
            access_class: soulseed_agi_core_models::AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        }
    }

    #[test]
    fn provider_passthrough_ok() {
        let provider = SoulbaseEnvProvider::new(OkTwClient);
        let ctx = provider.load_internal_scene(&anchor()).expect("scene");
        assert_eq!(ctx.conversation.scene, "demo");
    }

    #[test]
    fn provider_maps_error() {
        let provider = SoulbaseEnvProvider::new(FailingTwClient);
        let err = provider
            .load_internal_scene(&anchor())
            .expect_err("maps error");
        match err {
            EnvCtxError::SourceUnavailable { endpoint, reason } => {
                assert_eq!(endpoint, "envctx.internal_scene");
                assert!(reason.contains("timeout"));
            }
            _ => panic!("unexpected error"),
        }
    }
}

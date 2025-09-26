use crate::{
    compress::CompressorMock,
    config::ContextConfig,
    engine::ContextEngine,
    env_gateway::{EnvContextGateway, SoulbaseEnvGateway},
    errors::ContextError,
    obs::NoopObs,
    planner::DeterministicPlanner,
    pointer::PointerValidatorMock,
    qgate::QualityGateMock,
    score::ScoreAdapterSimple,
    store::InMemoryStore,
    traits::Observability,
    types::{Anchor, ContextItem, GraphExplain, RunInput, RunOutput},
};
use soulseed_agi_envctx::dto::{
    ConversationSummary, ExternalSystems, InteractionObject, InternalScene, LatencyWindow,
    LifeJourney, NetworkQuality, RiskLevel, SourceVersions, TaskSummary, ToolDefLite,
    ToolPermission, VersionPointer,
};
use soulseed_agi_envctx::{
    DegradationReporter, EnvCtxProvisioner, EnvironmentContext, EnvironmentDataProvider,
    NoopReporter, Result as EnvResult, ThinWaistClient,
};

/// Thin-waist 适配器，将 ContextEngine 运行所需的 mocks 统一封装，方便替换为真实实现。
pub struct ThinWaistContextAdapter {
    scorer: ScoreAdapterSimple,
    planner: DeterministicPlanner,
    compressor: CompressorMock,
    qgate: QualityGateMock,
    pointer: PointerValidatorMock,
    store: InMemoryStore,
    obs: NoopObs,
}

impl ThinWaistContextAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn engine(&self) -> ContextEngine<'_> {
        ContextEngine {
            scorer: &self.scorer,
            planner: &self.planner,
            compressor: &self.compressor,
            qgate: &self.qgate,
            pointer: &self.pointer,
            store: &self.store,
            obs: &self.obs,
        }
    }

    pub fn store(&self) -> &InMemoryStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut InMemoryStore {
        &mut self.store
    }

    pub fn quality_gate_mut(&mut self) -> &mut QualityGateMock {
        &mut self.qgate
    }
}

impl Default for ThinWaistContextAdapter {
    fn default() -> Self {
        Self {
            scorer: ScoreAdapterSimple,
            planner: DeterministicPlanner,
            compressor: CompressorMock,
            qgate: QualityGateMock::default(),
            pointer: PointerValidatorMock,
            store: InMemoryStore::default(),
            obs: NoopObs,
        }
    }
}

pub trait EnvAssembler {
    fn assemble_env(&self, anchor: &Anchor) -> EnvResult<EnvironmentContext>;
}

impl<P, R> EnvAssembler for EnvContextGateway<P, R>
where
    P: EnvironmentDataProvider,
    R: DegradationReporter,
{
    fn assemble_env(&self, anchor: &Anchor) -> EnvResult<EnvironmentContext> {
        self.assemble(anchor)
    }
}

#[derive(Clone, Debug)]
pub struct ContextRuntimeInput {
    pub anchor: Anchor,
    pub config: ContextConfig,
    pub items: Vec<ContextItem>,
    pub graph_explain: Option<GraphExplain>,
}

pub struct ContextRuntime<G, Obs = NoopObs> {
    env_gateway: G,
    scorer: ScoreAdapterSimple,
    planner: DeterministicPlanner,
    compressor: CompressorMock,
    qgate: QualityGateMock,
    pointer: PointerValidatorMock,
    store: InMemoryStore,
    obs: Obs,
}

impl<G, Obs> ContextRuntime<G, Obs>
where
    G: EnvAssembler,
    Obs: Observability,
{
    pub fn new(env_gateway: G, obs: Obs) -> Self {
        Self {
            env_gateway,
            scorer: ScoreAdapterSimple,
            planner: DeterministicPlanner,
            compressor: CompressorMock,
            qgate: QualityGateMock::default(),
            pointer: PointerValidatorMock,
            store: InMemoryStore::default(),
            obs,
        }
    }

    pub fn run(&self, input: ContextRuntimeInput) -> Result<RunOutput, ContextError> {
        let env_context = self
            .env_gateway
            .assemble_env(&input.anchor)
            .map_err(|err| ContextError::EnvAssembly(err.to_string()))?;

        let engine = ContextEngine {
            scorer: &self.scorer,
            planner: &self.planner,
            compressor: &self.compressor,
            qgate: &self.qgate,
            pointer: &self.pointer,
            store: &self.store,
            obs: &self.obs,
        };

        engine.run(RunInput {
            anchor: input.anchor,
            env_context,
            config: input.config,
            items: input.items,
            graph_explain: input.graph_explain,
        })
    }

    pub fn store(&self) -> &InMemoryStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut InMemoryStore {
        &mut self.store
    }

    pub fn quality_gate_mut(&mut self) -> &mut QualityGateMock {
        &mut self.qgate
    }
}

pub type LocalEnvGateway = EnvContextGateway<LocalEnvProvider, NoopReporter>;

pub type LocalContextRuntime = ContextRuntime<LocalEnvGateway, NoopObs>;

impl Default for LocalContextRuntime {
    fn default() -> Self {
        let gateway = LocalEnvGateway::default();
        Self::new(gateway, NoopObs)
    }
}

pub type SoulbaseContextRuntime<C> = ContextRuntime<SoulbaseEnvGateway<C>, NoopObs>;

impl<C> SoulbaseContextRuntime<C>
where
    C: ThinWaistClient,
{
    pub fn new_with_client(client: C) -> Self {
        let gateway = SoulbaseEnvGateway::with_client(client);
        Self::new(gateway, NoopObs)
    }
}

#[derive(Clone, Default)]
pub struct LocalEnvProvider;

impl EnvironmentDataProvider for LocalEnvProvider {
    fn load_internal_scene(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<InternalScene> {
        Ok(InternalScene {
            conversation: ConversationSummary {
                rounds: 1,
                topics: vec!["dialogue".into()],
                scene: "generic".into(),
            },
            task: TaskSummary {
                goal: "respond".into(),
                constraints: vec!["tokens<=800".into()],
            },
            latency_window: LatencyWindow {
                p50_ms: 120,
                p95_ms: 250,
            },
            risk_flag: RiskLevel::Low,
        })
    }

    fn load_external_systems(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<ExternalSystems> {
        Ok(ExternalSystems {
            environment: "dev".into(),
            region: "us-west-2".into(),
            timezone: "UTC".into(),
            locale: "en-US".into(),
            network_quality: NetworkQuality::Good,
            service_freshness: Vec::new(),
            policy_digest: "policy:local".into(),
        })
    }

    fn load_interaction_object(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<InteractionObject> {
        Ok(InteractionObject {
            human: None,
            ai_companions: Vec::new(),
            group: None,
        })
    }

    fn load_tool_permission(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<ToolPermission> {
        Ok(ToolPermission {
            available_tools: vec![ToolDefLite {
                tool_id: "noop".into(),
                version: "1".into(),
                capabilities: vec!["demo".into()],
                risk_level: Some("low".into()),
                scope: None,
            }],
            policy_digest: "tools:local".into(),
        })
    }

    fn load_life_journey(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<LifeJourney> {
        Ok(LifeJourney {
            milestones: Vec::new(),
            current_arc: Some("steady".into()),
        })
    }

    fn load_source_versions(
        &self,
        _anchor: &soulseed_agi_envctx::dto::Anchor,
    ) -> EnvResult<SourceVersions> {
        Ok(SourceVersions {
            graph_snapshot: VersionPointer {
                digest: "graph:local".into(),
                at: None,
            },
            policy_snapshot: VersionPointer {
                digest: "policy:local".into(),
                at: None,
            },
            observe_watermark: None,
        })
    }
}

impl Default for LocalEnvGateway {
    fn default() -> Self {
        EnvCtxProvisioner::new(LocalEnvProvider::default(), NoopReporter).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContextItem, FeatureVec, Partition};
    use soulseed_agi_core_models::{
        AccessClass, EventId, MessageId, Provenance, SessionId, TenantId,
    };
    use soulseed_agi_envctx::{
        dto::{Anchor as EnvAnchor, ConversationSummary, InteractionObject, LifeJourney},
        ThinWaistError,
    };
    use uuid::Uuid;

    #[derive(Clone)]
    struct HappyClient;

    impl ThinWaistClient for HappyClient {
        fn internal_scene(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<InternalScene, ThinWaistError> {
            Ok(InternalScene {
                conversation: ConversationSummary {
                    rounds: 1,
                    topics: vec!["happy".into()],
                    scene: "demo".into(),
                },
                task: TaskSummary {
                    goal: "respond".into(),
                    constraints: vec![],
                },
                latency_window: LatencyWindow {
                    p50_ms: 80,
                    p95_ms: 220,
                },
                risk_flag: RiskLevel::Low,
            })
        }

        fn external_systems(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<ExternalSystems, ThinWaistError> {
            Ok(ExternalSystems {
                environment: "prod".into(),
                region: "eu-central-1".into(),
                timezone: "UTC".into(),
                locale: "en-US".into(),
                network_quality: NetworkQuality::Excellent,
                service_freshness: Vec::new(),
                policy_digest: "policy:prod".into(),
            })
        }

        fn interaction_object(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<InteractionObject, ThinWaistError> {
            Ok(InteractionObject {
                human: None,
                ai_companions: Vec::new(),
                group: None,
            })
        }

        fn tool_permission(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<ToolPermission, ThinWaistError> {
            Ok(ToolPermission {
                available_tools: vec![ToolDefLite {
                    tool_id: "web.search".into(),
                    version: "1".into(),
                    capabilities: vec!["search".into()],
                    risk_level: Some("medium".into()),
                    scope: None,
                }],
                policy_digest: "tools:prod".into(),
            })
        }

        fn life_journey(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<LifeJourney, ThinWaistError> {
            Ok(LifeJourney {
                milestones: Vec::new(),
                current_arc: Some("serving".into()),
            })
        }

        fn source_versions(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<SourceVersions, ThinWaistError> {
            Ok(SourceVersions {
                graph_snapshot: VersionPointer {
                    digest: "graph:prod".into(),
                    at: None,
                },
                policy_snapshot: VersionPointer {
                    digest: "policy:prod".into(),
                    at: None,
                },
                observe_watermark: None,
            })
        }
    }

    #[derive(Clone)]
    struct DegradingClient;

    impl ThinWaistClient for DegradingClient {
        fn internal_scene(
            &self,
            anchor: &EnvAnchor,
        ) -> std::result::Result<InternalScene, ThinWaistError> {
            HappyClient.internal_scene(anchor)
        }

        fn external_systems(
            &self,
            _anchor: &EnvAnchor,
        ) -> std::result::Result<ExternalSystems, ThinWaistError> {
            Err(ThinWaistError::Failure {
                endpoint: "envctx.external_systems",
                message: "timeout".into(),
            })
        }

        fn interaction_object(
            &self,
            anchor: &EnvAnchor,
        ) -> std::result::Result<InteractionObject, ThinWaistError> {
            HappyClient.interaction_object(anchor)
        }

        fn tool_permission(
            &self,
            anchor: &EnvAnchor,
        ) -> std::result::Result<ToolPermission, ThinWaistError> {
            HappyClient.tool_permission(anchor)
        }

        fn life_journey(
            &self,
            anchor: &EnvAnchor,
        ) -> std::result::Result<LifeJourney, ThinWaistError> {
            HappyClient.life_journey(anchor)
        }

        fn source_versions(
            &self,
            anchor: &EnvAnchor,
        ) -> std::result::Result<SourceVersions, ThinWaistError> {
            HappyClient.source_versions(anchor)
        }
    }

    fn anchor() -> Anchor {
        Anchor {
            tenant_id: TenantId::new(1),
            envelope_id: Uuid::now_v7(),
            config_snapshot_hash: "cfg-prod".into(),
            config_snapshot_version: 2,
            session_id: Some(SessionId::new(99)),
            sequence_number: Some(5),
            access_class: AccessClass::Restricted,
            provenance: Some(Provenance {
                source: "graph".into(),
                method: "timeline".into(),
                model: None,
                content_digest_sha256: Some("sha256:ctx".into()),
            }),
            schema_v: 1,
            scenario: Some(crate::types::ConversationScenario::HumanToAi),
        }
    }

    fn item(anchor: &Anchor, id: &str, tokens: u32) -> ContextItem {
        ContextItem {
            anchor: anchor.clone(),
            id: id.into(),
            partition_hint: Some(Partition::P4Dialogue),
            source_event_id: EventId(u64::from(tokens)),
            source_message_id: Some(MessageId(u64::from(tokens))),
            content: serde_json::json!({"text": id}),
            tokens,
            features: FeatureVec {
                rel: 0.6,
                cau: 0.4,
                rec: 0.5,
                auth: 0.4,
                stab: 0.6,
                dup: 0.1,
                len: 0.5,
                risk: 0.1,
            },
            policy_tags: serde_json::json!({}),
            evidence: None,
        }
    }

    fn cfg(anchor: &Anchor) -> ContextConfig {
        let mut cfg = ContextConfig::default();
        cfg.snapshot_hash = anchor.config_snapshot_hash.clone();
        cfg.snapshot_version = anchor.config_snapshot_version;
        cfg.target_tokens = 200;
        cfg
    }

    #[test]
    fn runtime_assembles_with_thinwaist_client() {
        let runtime = SoulbaseContextRuntime::new_with_client(HappyClient);
        let anchor = anchor();
        let config = cfg(&anchor);
        let items = vec![item(&anchor, "hello", 120)];
        let result = runtime
            .run(ContextRuntimeInput {
                anchor,
                config,
                items,
                graph_explain: None,
            })
            .expect("run succeeds");
        assert!(result
            .bundle
            .explain
            .reasons
            .iter()
            .any(|r| r.contains("plan_applied")));
        assert!(result.env_snapshot.context_digest.starts_with("sha256:"));
        assert!(result.env_snapshot.snapshot_digest.starts_with("blake3:"));
        assert_eq!(
            result.env_snapshot.manifest_digest,
            result.manifest.manifest_digest
        );
        assert_eq!(
            result.env_snapshot.source_versions.graph_snapshot.digest,
            "graph:prod"
        );
        assert_eq!(
            result.env_snapshot.policy_digest.as_deref(),
            Some("policy:prod")
        );
        assert!(result.env_snapshot.evidence_pointers.is_empty());
        assert!(result.manifest.manifest_digest.starts_with("man-"));
    }

    #[test]
    fn runtime_records_env_degradation() {
        let runtime = SoulbaseContextRuntime::new_with_client(DegradingClient);
        let anchor = anchor();
        let config = cfg(&anchor);
        let items = vec![item(&anchor, "greet", 90)];
        let output = runtime
            .run(ContextRuntimeInput {
                anchor,
                config,
                items,
                graph_explain: None,
            })
            .expect("run degrades but succeeds");
        assert!(output
            .bundle
            .explain
            .reasons
            .iter()
            .any(|r| r.starts_with("envctx_degraded")));
        assert_eq!(
            output.bundle.explain.degradation_reason.as_deref(),
            Some("envctx:envctx.external_systems")
        );
        assert_eq!(
            output.env_snapshot.degradation_reason.as_deref(),
            Some("envctx.external_systems")
        );
        assert!(output.env_snapshot.snapshot_digest.starts_with("blake3:"));
        assert_eq!(
            output.env_snapshot.policy_digest.as_deref(),
            Some("unknown")
        );
    }
}

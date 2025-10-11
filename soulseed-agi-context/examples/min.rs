use serde_json::json;
use soulseed_agi_context::compress::CompressorMock;
use soulseed_agi_context::config::ContextConfig;
use soulseed_agi_context::engine::ContextEngine;
use soulseed_agi_context::obs::NoopObs;
use soulseed_agi_context::planner::DeterministicPlanner;
use soulseed_agi_context::pointer::PointerValidatorMock;
use soulseed_agi_context::qgate::QualityGateStrict;
use soulseed_agi_context::score::ScoreAdapterHalfLife;
use soulseed_agi_context::store::InMemoryStore;
use soulseed_agi_context::types::{
    AccessClass, Anchor, ContextItem, ContextItemDigests, ContextItemLinks, ConversationScenario,
    EventId, EvidencePointer, FeatureVec, GraphExplain, MessageId, Partition, Provenance, RunInput,
    SessionId, TenantId,
};
use soulseed_agi_envctx as envctx;
use time::OffsetDateTime;
use uuid::Uuid;

fn build_env_context(anchor: &Anchor) -> envctx::EnvironmentContext {
    let env_anchor = envctx::Anchor {
        tenant_id: anchor.tenant_id,
        envelope_id: anchor.envelope_id,
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id,
        sequence_number: anchor.sequence_number,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        schema_v: anchor.schema_v,
        supersedes: anchor.supersedes,
        superseded_by: anchor.superseded_by,
    };

    let mut ctx = envctx::EnvironmentContext {
        anchor: env_anchor,
        internal_scene: envctx::InternalScene {
            conversation: envctx::ConversationSummary {
                rounds: 2,
                topics: vec!["demo".into()],
                scene: "clarify".into(),
            },
            task: envctx::TaskSummary {
                goal: "demo_goal".into(),
                constraints: vec!["latency<=250".into()],
            },
            latency_window: envctx::LatencyWindow::new(80, 210),
            risk_flag: envctx::RiskLevel::Low,
        },
        external_systems: envctx::ExternalSystems {
            environment: "dev".into(),
            region: "us-east-1".into(),
            timezone: "UTC".into(),
            locale: "en-US".into(),
            network_quality: envctx::NetworkQuality::Good,
            service_freshness: vec![envctx::ServiceFreshness {
                name: "graph".into(),
                freshness: envctx::FreshnessState::Fresh,
                last_synced_at: Some(OffsetDateTime::now_utc()),
            }],
            policy_digest: "policy:v1".into(),
        },
        interaction_object: envctx::InteractionObject {
            human: None,
            ai_companions: Vec::new(),
            group: None,
        },
        tool_permission: envctx::ToolPermission {
            available_tools: vec![envctx::ToolDefLite {
                tool_id: "web.search".into(),
                version: "1".into(),
                capabilities: vec!["search".into()],
                risk_level: Some("medium".into()),
                scope: Some("demo".into()),
            }],
            policy_digest: "tools:v1".into(),
        },
        life_journey: envctx::LifeJourney {
            milestones: vec![envctx::LifeMilestone {
                name: "boot".into(),
                occurred_at: OffsetDateTime::now_utc(),
                significance: Some("init".into()),
            }],
            current_arc: Some("serving".into()),
        },
        source_versions: envctx::SourceVersions {
            graph_snapshot: envctx::VersionPointer {
                digest: "graph:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            },
            policy_snapshot: envctx::VersionPointer {
                digest: "policy:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            },
            tool_catalog_snapshot: Some(envctx::VersionPointer {
                digest: "tools:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            }),
            authz_snapshot: Some(envctx::VersionPointer {
                digest: "authz:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            }),
            quota_snapshot: None,
            observe_watermark: None,
            monitoring_snapshot: None,
        },
        environment_vectors: Vec::new(),
        navigation_path: None,
        context_digest: String::new(),
        degradation_reason: None,
        lite_mode: false,
    };
    ctx.context_digest = envctx::compute_digest(&ctx).expect("digest");
    ctx
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anchor = Anchor {
        tenant_id: TenantId(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg-1".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(42)),
        sequence_number: Some(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "graph".into(),
            method: "ingest".into(),
            model: None,
            content_digest_sha256: Some("sha256:demo".into()),
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    };

    let item = ContextItem {
        anchor: anchor.clone(),
        id: "ci-1".into(),
        partition: Partition::P4Dialogue,
        partition_hint: Some(Partition::P4Dialogue),
        source_event_id: EventId::from_raw_unchecked(100),
        source_message_id: Some(MessageId(55)),
        observed_at: OffsetDateTime::UNIX_EPOCH,
        content: json!({"text": "Hello world"}),
        tokens: 150,
        features: FeatureVec {
            rel: 0.7,
            cau: 0.4,
            rec: 0.6,
            auth: 0.5,
            stab: 0.8,
            dup: 0.1,
            len: 0.4,
            risk: 0.2,
        },
        policy_tags: json!({"scene": "chat"}),
        typ: Some("dialogue".into()),
        digests: ContextItemDigests {
            content: Some("sha256:dialogue".into()),
            ..Default::default()
        },
        links: ContextItemLinks {
            evidence_ptrs: vec![EvidencePointer {
                uri: "s3://demo/context/ci-1.json".into(),
                digest_sha256: Some("sha256:abcdef".into()),
                media_type: Some("application/json".into()),
                blob_ref: None,
                span: Some((0, 10)),
                access_policy: Some("restricted".into()),
            }],
            supersedes: None,
        },
    };

    let env_context = build_env_context(&anchor);

    let mut config = ContextConfig::default();
    config.snapshot_hash = anchor.config_snapshot_hash.clone();
    config.snapshot_version = anchor.config_snapshot_version;
    config.target_tokens = 100;
    config.plan_seed = 42;

    let engine = ContextEngine {
        scorer: &ScoreAdapterHalfLife,
        planner: &DeterministicPlanner,
        compressor: &CompressorMock,
        qgate: &QualityGateStrict::default(),
        pointer: &PointerValidatorMock,
        store: &InMemoryStore::default(),
        obs: &NoopObs,
    };

    let output = engine.run(RunInput {
        anchor: anchor.clone(),
        env_context,
        config,
        items: vec![item],
        graph_explain: Some(GraphExplain {
            reasons: vec!["graph_cache_hit".into()],
            indices_used: vec!["timeline_v1".into()],
            query_hash: Some("timeline:tenant:42".into()),
            degradation_reason: Some("graph_sparse_only".into()),
        }),
        previous_manifest: None,
    })?;

    println!("plan_id: {}", output.plan.plan_id);
    println!("tokens_saved: {}", output.report.tokens_saved);
    println!(
        "degradation: {:?}",
        output.bundle.explain.degradation_reason
    );

    Ok(())
}

use super::*;
use crate::errors::{EnvCtxError, Result};
use crate::facade::{DegradationReporter, EnvironmentDataProvider};
use soulseed_agi_core_models::{AIId, AccessClass, HumanId, Provenance, SessionId, TenantId};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

struct FakeProvider {
    fail_external: bool,
}

impl FakeProvider {
    fn new() -> Self {
        Self {
            fail_external: false,
        }
    }

    fn with_external_failure() -> Self {
        Self {
            fail_external: true,
        }
    }
}

impl EnvironmentDataProvider for FakeProvider {
    fn load_internal_scene(&self, _anchor: &Anchor) -> Result<InternalScene> {
        Ok(InternalScene {
            conversation: ConversationSummary {
                rounds: 3,
                topics: vec!["planning".into()],
                scene: "clarify".into(),
            },
            task: TaskSummary {
                goal: "answer question".into(),
                constraints: vec!["token<=800".into()],
            },
            latency_window: LatencyWindow {
                p50_ms: 80,
                p95_ms: 220,
            },
            risk_flag: RiskLevel::Low,
        })
    }

    fn load_external_systems(&self, _anchor: &Anchor) -> Result<ExternalSystems> {
        if self.fail_external {
            return Err(EnvCtxError::SourceUnavailable {
                endpoint: "graph",
                reason: "timeout".into(),
            });
        }
        Ok(ExternalSystems {
            environment: "prod".into(),
            region: "us-east-1".into(),
            timezone: "UTC".into(),
            locale: "en-US".into(),
            network_quality: NetworkQuality::Excellent,
            service_freshness: vec![ServiceFreshness {
                name: "graph".into(),
                freshness: FreshnessState::Fresh,
                last_synced_at: Some(OffsetDateTime::now_utc()),
            }],
            policy_digest: "policy:v1".into(),
        })
    }

    fn load_interaction_object(&self, _anchor: &Anchor) -> Result<InteractionObject> {
        Ok(InteractionObject {
            human: Some(HumanView {
                id: HumanId::new(41),
                role: "member".into(),
                scope: "tenant".into(),
            }),
            ai_companions: vec![AIView {
                id: AIId::new(9001),
                soul_state: SoulState::Active,
            }],
            group: None,
        })
    }

    fn load_tool_permission(&self, _anchor: &Anchor) -> Result<ToolPermission> {
        Ok(ToolPermission {
            available_tools: vec![ToolDefLite {
                tool_id: "web.search".into(),
                version: "1".into(),
                capabilities: vec!["search".into()],
                risk_level: Some("medium".into()),
                scope: None,
            }],
            policy_digest: "tools:v1".into(),
        })
    }

    fn load_life_journey(&self, _anchor: &Anchor) -> Result<LifeJourney> {
        Ok(LifeJourney {
            milestones: vec![LifeMilestone {
                name: "bootstrap".into(),
                occurred_at: OffsetDateTime::now_utc(),
                significance: Some("init".into()),
            }],
            current_arc: Some("serving".into()),
        })
    }

    fn load_source_versions(&self, _anchor: &Anchor) -> Result<SourceVersions> {
        Ok(SourceVersions {
            graph_snapshot: VersionPointer {
                digest: "graph:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            },
            policy_snapshot: VersionPointer {
                digest: "policy:v1".into(),
                at: Some(OffsetDateTime::now_utc()),
            },
            observe_watermark: None,
        })
    }
}

#[derive(Clone, Default)]
struct CollectingReporter {
    entries: Arc<Mutex<Vec<(String, Option<String>)>>>,
}

impl DegradationReporter for CollectingReporter {
    fn report_degradation(&self, _anchor: &Anchor, code: &str, detail: Option<&str>) {
        self.entries
            .lock()
            .unwrap()
            .push((code.to_string(), detail.map(|d| d.to_string())));
    }
}

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId::new(1),
        envelope_id: uuid::Uuid::now_v7(),
        config_snapshot_hash: "cfg-hash".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::new(77)),
        sequence_number: Some(10),
        access_class: AccessClass::Internal,
        provenance: Some(Provenance {
            source: "envctx-tests".into(),
            method: "unit".into(),
            model: None,
            content_digest_sha256: None,
        }),
        schema_v: 1,
    }
}

#[test]
fn assemble_success() {
    let provider = FakeProvider::new();
    let reporter = CollectingReporter::default();
    let engine = EnvironmentEngine::new(provider, reporter.clone());
    let ctx = engine.assemble(anchor()).expect("envctx");
    assert!(ctx.context_digest.starts_with("sha256:"));
    assert!(ctx.degradation_reason.is_none());
    assert!(reporter.entries.lock().unwrap().is_empty());
}

#[test]
fn assemble_with_external_fallback() {
    let provider = FakeProvider::with_external_failure();
    let reporter = CollectingReporter::default();
    let engine = EnvironmentEngine::new(provider, reporter.clone());
    let ctx = engine.assemble(anchor()).expect("envctx fallback");
    assert_eq!(ctx.external_systems.environment, "unknown");
    assert!(ctx.degradation_reason.is_some());
    let entries = reporter.entries.lock().unwrap();
    assert_eq!(entries[0].0, "envctx.external_systems");
}

#[test]
fn reject_invalid_anchor() {
    let provider = FakeProvider::new();
    let reporter = CollectingReporter::default();
    let engine = EnvironmentEngine::new(provider, reporter);
    let mut bad = anchor();
    bad.config_snapshot_hash.clear();
    let err = engine.assemble(bad).unwrap_err();
    assert!(matches!(err, EnvCtxError::Missing("config_snapshot_hash")));
}

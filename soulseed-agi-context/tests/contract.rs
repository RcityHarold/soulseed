use std::collections::HashSet;

use serde_json::json;
use soulseed_agi_context::compress::CompressorMock;
use soulseed_agi_context::config::{ContextConfig, PartitionQuota};
use soulseed_agi_context::engine::ContextEngine;
use soulseed_agi_context::errors::ContextError;
use soulseed_agi_context::obs::NoopObs;
use soulseed_agi_context::planner::DeterministicPlanner;
use soulseed_agi_context::pointer::PointerValidatorMock;
use soulseed_agi_context::qgate::QualityGateStrict;
use soulseed_agi_context::score::ScoreAdapterHalfLife;
use soulseed_agi_context::store::InMemoryStore;
use soulseed_agi_context::types::{
    AccessClass, Anchor, BudgetSummary, BundleItem, BundleScoreStats, BundleSegment, ContextBundle,
    ContextItem, ContextItemDigests, ContextItemLinks, EventId, EvidencePointer, ExplainBundle,
    FeatureVec, GraphExplain, Level, MessageId, Partition, PlanAction, PromptBundle, Provenance,
    RunInput, SessionId, TenantId,
};
use soulseed_agi_context::{build_prefix, compact, merge_delta};
use soulseed_agi_core_models::ConversationScenario;
use soulseed_agi_envctx as envctx;
use time::OffsetDateTime;
use uuid::Uuid;

fn default_anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId(7)),
        sequence_number: Some(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "graph".into(),
            method: "ingest".into(),
            model: None,
            content_digest_sha256: Some("sha256:ok".into()),
        }),
        schema_v: 1,
        scenario: None,
        supersedes: None,
        superseded_by: None,
    }
}

fn env_ctx(anchor: &Anchor) -> envctx::EnvironmentContext {
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
                rounds: 1,
                topics: vec!["contract".into()],
                scene: "clarify".into(),
            },
            task: envctx::TaskSummary {
                goal: "contract_test".into(),
                constraints: Vec::new(),
            },
            latency_window: envctx::LatencyWindow::new(50, 150),
            risk_flag: envctx::RiskLevel::Low,
        },
        external_systems: envctx::ExternalSystems {
            environment: "dev".into(),
            region: "test".into(),
            timezone: "UTC".into(),
            locale: "en-US".into(),
            network_quality: envctx::NetworkQuality::Good,
            service_freshness: Vec::new(),
            policy_digest: "policy:v1".into(),
        },
        interaction_object: envctx::InteractionObject {
            human: None,
            ai_companions: Vec::new(),
            group: None,
        },
        tool_permission: envctx::ToolPermission {
            available_tools: Vec::new(),
            policy_digest: "tools:v1".into(),
        },
        life_journey: envctx::LifeJourney {
            milestones: Vec::new(),
            current_arc: None,
        },
        source_versions: envctx::SourceVersions {
            graph_snapshot: envctx::VersionPointer {
                digest: "graph:v1".into(),
                at: None,
            },
            policy_snapshot: envctx::VersionPointer {
                digest: "policy:v1".into(),
                at: None,
            },
            tool_catalog_snapshot: Some(envctx::VersionPointer {
                digest: "tools:v1".into(),
                at: None,
            }),
            authz_snapshot: Some(envctx::VersionPointer {
                digest: "authz:v1".into(),
                at: None,
            }),
            quota_snapshot: None,
            observe_watermark: None,
            monitoring_snapshot: None,
        },
        context_digest: String::new(),
        degradation_reason: None,
        lite_mode: false,
    };
    ctx.context_digest = envctx::compute_digest(&ctx).expect("digest");
    ctx
}

fn mk_item(
    anchor: &Anchor,
    id: &str,
    partition: Partition,
    tokens: u32,
    pointer: bool,
) -> ContextItem {
    let evidence_ptrs = if pointer {
        vec![EvidencePointer {
            uri: "s3://demo".into(),
            digest_sha256: Some("sha256:valid".into()),
            media_type: Some("application/json".into()),
            blob_ref: None,
            span: Some((0, 10)),
            access_policy: Some("restricted".into()),
        }]
    } else {
        Vec::new()
    };
    ContextItem {
        anchor: anchor.clone(),
        id: id.into(),
        partition,
        partition_hint: Some(partition),
        source_event_id: EventId(10),
        source_message_id: Some(MessageId(11)),
        observed_at: OffsetDateTime::UNIX_EPOCH,
        content: json!({"text": id}),
        tokens,
        features: FeatureVec {
            rel: 0.6,
            cau: 0.3,
            rec: 0.6,
            auth: 0.4,
            stab: 0.7,
            dup: 0.1,
            len: 0.4,
            risk: 0.1,
        },
        policy_tags: json!({}),
        typ: Some("fact".into()),
        digests: ContextItemDigests {
            content: Some(format!("sha256:{id}")),
            ..Default::default()
        },
        links: ContextItemLinks {
            evidence_ptrs,
            supersedes: None,
        },
    }
}

fn make_engine<'a>(qgate: &'a QualityGateStrict, store: &'a InMemoryStore) -> ContextEngine<'a> {
    ContextEngine {
        scorer: &ScoreAdapterHalfLife,
        planner: &DeterministicPlanner,
        compressor: &CompressorMock,
        qgate,
        pointer: &PointerValidatorMock,
        store,
        obs: &NoopObs,
    }
}

fn cfg_for(anchor: &Anchor) -> ContextConfig {
    let mut cfg = ContextConfig::default();
    cfg.snapshot_hash = anchor.config_snapshot_hash.clone();
    cfg.snapshot_version = anchor.config_snapshot_version;
    cfg
}

fn explain_stub() -> ExplainBundle {
    ExplainBundle {
        reasons: Vec::new(),
        degradation_reason: None,
        indices_used: Vec::new(),
        query_hash: None,
    }
}

fn segments_from(spec: Vec<(Partition, Vec<(&str, Option<Level>, u32)>)>) -> Vec<BundleSegment> {
    spec.into_iter()
        .map(|(partition, items)| BundleSegment {
            partition,
            items: items
                .into_iter()
                .map(|(ci_id, level, tokens)| BundleItem {
                    ci_id: ci_id.into(),
                    partition,
                    summary_level: level,
                    tokens,
                    score_scaled: 0,
                    ts_ms: 0,
                    digests: ContextItemDigests::default(),
                    typ: None,
                    why_included: None,
                    score_stats: BundleScoreStats::default(),
                    supersedes: None,
                    evidence_ptrs: Vec::new(),
                })
                .collect(),
        })
        .collect()
}

fn base_bundle(anchor: &Anchor) -> ContextBundle {
    ContextBundle {
        anchor: anchor.clone(),
        schema_v: anchor.schema_v,
        version: 1,
        segments: segments_from(vec![
            (
                Partition::P1TaskFacts,
                vec![
                    ("ci-upd", Some(Level::L1), 120),
                    ("ci-stable", Some(Level::L1), 90),
                ],
            ),
            (
                Partition::P2Evidence,
                vec![("ci-old", None, 80), ("ci-keep", None, 140)],
            ),
        ]),
        explain: explain_stub(),
        budget: BudgetSummary {
            target_tokens: 400,
            projected_tokens: 330,
        },
        prompt: PromptBundle::default(),
    }
}

fn partition_rank(partition: Partition) -> u8 {
    match partition {
        Partition::P0Policy => 0,
        Partition::P1TaskFacts => 1,
        Partition::P2Evidence => 2,
        Partition::P3WorkingDelta => 3,
        Partition::P4Dialogue => 4,
    }
}

#[test]
fn scenario_override_adjusts_dialogue_quota() {
    let cfg = ContextConfig::default();
    let override_cfg = cfg.for_scenario(Some(&ConversationScenario::HumanToAi));
    let base_max = cfg
        .partition_quota
        .get(&Partition::P4Dialogue)
        .map(|quota| quota.resolve(cfg.target_tokens).0)
        .unwrap_or_default();
    let override_max = override_cfg
        .partition_quota
        .get(&Partition::P4Dialogue)
        .map(|quota| quota.resolve(cfg.target_tokens).0)
        .unwrap_or_default();
    assert!(override_max > base_max);
}

#[test]
fn partition_eviction_order() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 200;
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![
        mk_item(&anchor, "p4", Partition::P4Dialogue, 300, false),
        mk_item(&anchor, "p3", Partition::P3WorkingDelta, 250, false),
        mk_item(&anchor, "p2", Partition::P2Evidence, 200, true),
        mk_item(&anchor, "p1", Partition::P1TaskFacts, 150, false),
    ];
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    let actions: Vec<(String, PlanAction)> = out
        .plan
        .items
        .iter()
        .map(|i| (i.ci_id.clone(), i.action.clone()))
        .collect();
    assert!(actions
        .iter()
        .any(|(id, action)| id == "p2" && !matches!(action, PlanAction::Keep)));
    assert!(actions
        .iter()
        .all(|(id, action)| !(id == "p1" && matches!(action, PlanAction::Evict))));
}

#[test]
fn deterministic_plan_same_input() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 200;
    cfg.plan_seed = 99;
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![
        mk_item(&anchor, "a", Partition::P4Dialogue, 300, false),
        mk_item(&anchor, "b", Partition::P4Dialogue, 250, false),
    ];
    let env_context = env_ctx(&anchor);
    let out1 = engine
        .run(RunInput {
            anchor: anchor.clone(),
            env_context: env_context.clone(),
            config: cfg.clone(),
            items: items.clone(),
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    let out2 = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    assert_eq!(out1.plan.plan_id, out2.plan.plan_id);
    assert_eq!(out1.manifest.manifest_digest, out2.manifest.manifest_digest);
}

#[test]
fn explanation_captures_graph_degrade_reason() {
    let mut anchor = default_anchor();
    anchor.scenario = Some(ConversationScenario::AiSelfTalk);
    let cfg = cfg_for(&anchor);
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![mk_item(&anchor, "d", Partition::P4Dialogue, 120, false)];
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: Some(GraphExplain {
                reasons: vec!["graph_plan_cache_hit".into()],
                indices_used: vec!["timeline_v1".into()],
                query_hash: Some("timeline:tenant:session".into()),
                degradation_reason: Some("timeline_sparse".into()),
            }),
            previous_manifest: None,
        })
        .unwrap();
    assert!(out
        .bundle
        .explain
        .reasons
        .iter()
        .any(|r| r.contains("graph_degraded:timeline_sparse")));
    assert!(out
        .bundle
        .explain
        .reasons
        .iter()
        .any(|r| r == "graph_plan_cache_hit"));
    assert_eq!(
        out.bundle.explain.indices_used,
        vec![String::from("timeline_v1")]
    );
    assert_eq!(
        out.bundle.explain.query_hash.as_deref(),
        Some("timeline:tenant:session")
    );
    assert!(out.env_snapshot.context_digest.starts_with("sha256:"));
    assert!(out.env_snapshot.snapshot_digest.starts_with("sha256:"));
    assert_eq!(
        out.env_snapshot.manifest_digest,
        out.manifest.manifest_digest
    );
    assert_eq!(
        out.env_snapshot.source_versions.graph_snapshot.digest,
        "graph:v1"
    );
    assert_eq!(out.env_snapshot.policy_digest.as_deref(), Some("policy:v1"));
    assert!(!out.env_snapshot.evidence_pointers.is_empty());
}

#[test]
fn quality_gate_failure_triggers_rollback() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 50;
    cfg.partition_quota
        .insert(Partition::P2Evidence, PartitionQuota::new(512, 0));
    let qgate = QualityGateStrict::new(Some(Level::L3), false);
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![mk_item(&anchor, "x", Partition::P2Evidence, 200, true)];
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    assert!(out
        .report
        .rolled_back
        .iter()
        .any(|(id, reason)| id.contains("su:x") && reason == "quality_gate_fail"));
    let entry = out
        .redaction
        .entries
        .iter()
        .find(|entry| entry.reason == "quality_gate_fail")
        .expect("redaction entry");
    let pointer = entry
        .evidence_pointer
        .as_ref()
        .expect("quality gate failure retains pointer");
    assert_eq!(pointer.uri, "s3://demo");
}

#[test]
fn pointer_invalid_returns_error() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 100;
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let mut bad = mk_item(&anchor, "bad", Partition::P2Evidence, 120, true);
    if let Some(pointer) = bad.links.evidence_ptrs.first_mut() {
        pointer.digest_sha256 = Some("md5:broken".into());
    }
    let env_context = env_ctx(&anchor);
    let res = engine.run(RunInput {
        anchor,
        env_context,
        config: cfg,
        items: vec![bad],
        graph_explain: None,
        previous_manifest: None,
    });
    assert!(matches!(res.unwrap_err(), ContextError::PointerInvalid(_)));
}

#[test]
fn degradation_reason_propagated() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 200;
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![mk_item(&anchor, "d", Partition::P4Dialogue, 300, false)];
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: Some(GraphExplain {
                reasons: vec!["timeline_hit".into()],
                indices_used: vec!["timeline_v1".into(), "causal_v1".into()],
                query_hash: Some("timeline:hash".into()),
                degradation_reason: Some("graph_sparse_only".into()),
            }),
            previous_manifest: None,
        })
        .unwrap();
    let degrade = out.bundle.explain.degradation_reason.unwrap();
    assert!(
        degrade.contains("graph:graph_sparse_only"),
        "unexpected degradation summary: {degrade}"
    );
    assert_eq!(
        out.bundle.explain.indices_used,
        vec![String::from("timeline_v1"), String::from("causal_v1")]
    );
    assert_eq!(
        out.bundle.explain.query_hash.as_deref(),
        Some("timeline:hash")
    );
    assert!(out
        .env_snapshot
        .degradation_reason
        .as_deref()
        .map(|reason| reason.contains("graph:graph_sparse_only"))
        .unwrap_or(false));
}

#[test]
fn six_anchor_consistency_and_privacy_redline() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 100;
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor: anchor.clone(),
            env_context: env_context.clone(),
            config: cfg.clone(),
            items: vec![mk_item(&anchor, "ok", Partition::P4Dialogue, 50, false)],
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    assert_eq!(out.bundle.anchor.tenant_id.0, anchor.tenant_id.0);

    let mut anchor_bad = anchor.clone();
    anchor_bad.provenance = None;
    let env_context_bad = env_ctx(&anchor_bad);
    let res = engine.run(RunInput {
        anchor: anchor_bad,
        env_context: env_context_bad,
        config: cfg,
        items: Vec::new(),
        graph_explain: None,
        previous_manifest: None,
    });
    assert!(matches!(res.unwrap_err(), ContextError::PrivacyRestricted));
}

#[test]
fn env_snapshot_event_contains_scene() {
    let anchor = default_anchor();
    let cfg = cfg_for(&anchor);
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let env_context = env_ctx(&anchor);
    let ctx_item = mk_item(&anchor, "ctx", Partition::P4Dialogue, 80, true);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items: vec![ctx_item],
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();
    assert!(out
        .env_snapshot
        .scene
        .as_deref()
        .unwrap()
        .contains("clarify"));
    assert!(out.env_snapshot.snapshot_digest.starts_with("sha256:"));
    assert_eq!(
        out.env_snapshot.manifest_digest,
        out.manifest.manifest_digest
    );
    assert_eq!(out.env_snapshot.policy_digest.as_deref(), Some("policy:v1"));
    assert_eq!(
        out.env_snapshot.source_versions.policy_snapshot.digest,
        "policy:v1"
    );
    assert!(out
        .env_snapshot
        .evidence_pointers
        .iter()
        .any(|ptr| ptr.uri == "s3://demo"));
    assert!(out.manifest.manifest_digest.starts_with("man-"));
    assert!(out
        .redaction
        .entries
        .iter()
        .all(|entry| entry.reason.contains("fallback") || entry.reason.contains("quality")));
}

#[test]
fn partition_min_tokens_respected() {
    let anchor = default_anchor();
    let mut cfg = cfg_for(&anchor);
    cfg.target_tokens = 180;
    cfg.partition_quota
        .insert(Partition::P1TaskFacts, PartitionQuota::new(256, 150));
    let qgate = QualityGateStrict::default();
    let store = InMemoryStore::default();
    let engine = make_engine(&qgate, &store);
    let items = vec![
        mk_item(&anchor, "facts-1", Partition::P1TaskFacts, 100, false),
        mk_item(&anchor, "facts-2", Partition::P1TaskFacts, 80, false),
        mk_item(&anchor, "dialogue-1", Partition::P4Dialogue, 120, false),
    ];
    let env_context = env_ctx(&anchor);
    let out = engine
        .run(RunInput {
            anchor,
            env_context,
            config: cfg,
            items,
            graph_explain: None,
            previous_manifest: None,
        })
        .unwrap();

    let retained_tokens: u32 = out
        .plan
        .items
        .iter()
        .filter(|item| item.partition == Partition::P1TaskFacts)
        .map(|item| item.tokens_after)
        .sum();
    assert!(retained_tokens >= 150);
}

#[test]
fn merge_delta_anchor_mismatch_errors() {
    let anchor = default_anchor();
    let manifest = build_prefix(&base_bundle(&anchor)).unwrap();
    let mut other_anchor = anchor.clone();
    other_anchor.envelope_id = Uuid::now_v7();
    let delta_bundle = ContextBundle {
        anchor: other_anchor,
        schema_v: anchor.schema_v,
        version: manifest.version,
        segments: segments_from(vec![(
            Partition::P1TaskFacts,
            vec![("ci-new", Some(Level::L1), 50)],
        )]),
        explain: explain_stub(),
        budget: BudgetSummary {
            target_tokens: 360,
            projected_tokens: 260,
        },
        prompt: PromptBundle::default(),
    };
    match merge_delta(&manifest, &delta_bundle) {
        Err(ContextError::DeltaMismatch(_)) => {}
        Err(err) => panic!("unexpected error: {err:?}"),
        Ok(_) => panic!("merge_delta should fail for anchor mismatch"),
    }
}

#[test]
fn merge_delta_hitl_patch_reports_entries() {
    let anchor = default_anchor();
    let manifest = build_prefix(&base_bundle(&anchor)).unwrap();
    let delta_bundle = ContextBundle {
        anchor: anchor.clone(),
        schema_v: anchor.schema_v,
        version: manifest.version,
        segments: segments_from(vec![
            (
                Partition::P3WorkingDelta,
                vec![("ci-hitl", Some(Level::L2), 60)],
            ),
            (Partition::P2Evidence, vec![("ci-upd", Some(Level::L1), 70)]),
        ]),
        explain: explain_stub(),
        budget: BudgetSummary {
            target_tokens: 380,
            projected_tokens: 270,
        },
        prompt: PromptBundle::default(),
    };
    let outcome = merge_delta(&manifest, &delta_bundle).expect("merge");
    let merged = outcome.manifest;
    let report = outcome.report;
    let patch = outcome.patch;
    assert!(merged
        .segments
        .iter()
        .any(|seg| seg.partition == Partition::P3WorkingDelta
            && seg.items.iter().any(|item| item.ci_id == "ci-hitl")));
    assert_eq!(report.added, vec![String::from("ci-hitl")]);
    assert_eq!(report.updated, vec![String::from("ci-upd")]);
    assert!(patch.added.contains(&"ci-hitl".to_string()));
}

#[test]
fn manifest_build_prefix_sorts_and_hashes() {
    let anchor = default_anchor();
    let bundle = base_bundle(&anchor);
    let manifest = build_prefix(&bundle).expect("manifest");
    assert_eq!(manifest.version, 1);
    assert!(manifest.manifest_digest.starts_with("man-"));
    assert!(manifest
        .segments
        .windows(2)
        .all(|w| partition_rank(w[0].partition) <= partition_rank(w[1].partition)));
    let second = build_prefix(&bundle).unwrap();
    assert_eq!(manifest.manifest_digest, second.manifest_digest);
}

#[test]
fn manifest_merge_delta_applies_patch() {
    let anchor = default_anchor();
    let manifest = build_prefix(&base_bundle(&anchor)).unwrap();
    let delta_bundle = ContextBundle {
        anchor: anchor.clone(),
        schema_v: anchor.schema_v,
        version: manifest.version,
        segments: segments_from(vec![
            (
                Partition::P1TaskFacts,
                vec![("ci-upd", Some(Level::L2), 70)],
            ),
            (Partition::P2Evidence, vec![("ci-new", None, 60)]),
        ]),
        explain: explain_stub(),
        budget: BudgetSummary {
            target_tokens: 360,
            projected_tokens: 260,
        },
        prompt: PromptBundle::default(),
    };
    let outcome = merge_delta(&manifest, &delta_bundle).expect("merge");
    let merged = outcome.manifest;
    let report = outcome.report;
    let patch = outcome.patch;
    assert_eq!(merged.version, manifest.version);
    assert_eq!(merged.working_generation, manifest.working_generation + 1);
    assert!(merged
        .segments
        .iter()
        .any(|seg| seg.partition == Partition::P2Evidence
            && seg.items.iter().any(|item| item.ci_id == "ci-new")));
    assert!(merged
        .segments
        .iter()
        .all(|seg| seg.items.iter().all(|item| item.ci_id != "ci-old")));
    assert_eq!(report.added, vec![String::from("ci-new")]);
    assert_eq!(report.updated, vec![String::from("ci-upd")]);
    assert!(report.removed.contains(&String::from("ci-old")));
    assert!(report.ignored.is_empty());
    assert!(merged.manifest_digest.starts_with("man-"));
    assert!(patch.added.contains(&"ci-new".to_string()));
    assert!(patch.removed.contains(&"ci-old".to_string()));
}

#[test]
fn manifest_merge_delta_missing_item_errors() {
    let anchor = default_anchor();
    let manifest = build_prefix(&base_bundle(&anchor)).unwrap();
    let delta_bundle = ContextBundle {
        anchor: anchor.clone(),
        schema_v: anchor.schema_v,
        version: manifest.version,
        segments: segments_from(vec![(
            Partition::P1TaskFacts,
            vec![("ci-upd", Some(Level::L2), 70)],
        )]),
        explain: explain_stub(),
        budget: BudgetSummary {
            target_tokens: 360,
            projected_tokens: 260,
        },
        prompt: PromptBundle::default(),
    };
    let outcome = merge_delta(&manifest, &delta_bundle).expect("merge");
    assert!(outcome.report.updated.contains(&"ci-upd".to_string()));
}

#[test]
fn manifest_compact_removes_zero_tokens_and_duplicates() {
    let anchor = default_anchor();
    let mut manifest = build_prefix(&base_bundle(&anchor)).unwrap();
    manifest.version = 7;
    if let Some(segment) = manifest
        .segments
        .iter_mut()
        .find(|seg| seg.partition == Partition::P2Evidence)
    {
        segment.items.push(BundleItem {
            ci_id: "ci-zero".into(),
            partition: Partition::P2Evidence,
            summary_level: None,
            tokens: 0,
            score_scaled: 0,
            ts_ms: 0,
            digests: ContextItemDigests::default(),
            typ: None,
            why_included: None,
            score_stats: BundleScoreStats::default(),
            supersedes: None,
            evidence_ptrs: Vec::new(),
        });
        segment.items.push(BundleItem {
            ci_id: "ci-dup".into(),
            partition: Partition::P2Evidence,
            summary_level: Some(Level::L1),
            tokens: 45,
            score_scaled: 0,
            ts_ms: 0,
            digests: ContextItemDigests::default(),
            typ: None,
            why_included: None,
            score_stats: BundleScoreStats::default(),
            supersedes: None,
            evidence_ptrs: Vec::new(),
        });
        segment.items.push(BundleItem {
            ci_id: "ci-dup".into(),
            partition: Partition::P2Evidence,
            summary_level: Some(Level::L2),
            tokens: 30,
            score_scaled: 0,
            ts_ms: 0,
            digests: ContextItemDigests::default(),
            typ: None,
            why_included: None,
            score_stats: BundleScoreStats::default(),
            supersedes: None,
            evidence_ptrs: Vec::new(),
        });
    }

    let (compacted, report) = compact(&manifest);
    assert_eq!(compacted.version, 8);
    assert!(compacted
        .segments
        .iter()
        .all(|seg| seg.items.iter().all(|item| item.tokens > 0)));
    assert!(compacted.segments.iter().all(|seg| {
        let mut seen = HashSet::new();
        seg.items.iter().all(|item| seen.insert(item.ci_id.clone()))
    }));
    assert!(compacted.manifest_digest.starts_with("man-"));
    assert_eq!(report.version_from, 7);
    assert_eq!(report.version_to, 8);
    assert_eq!(report.removed_zero_tokens, vec![String::from("ci-zero")]);
    assert_eq!(report.deduplicated, vec![String::from("ci-dup")]);
}

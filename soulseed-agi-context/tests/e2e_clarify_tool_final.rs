use serde_json::json;
use soulseed_agi_context::config::ContextConfig;
use soulseed_agi_context::thinwaist::{ContextRuntimeInput, LocalContextRuntime};
use soulseed_agi_context::types::{
    AccessClass, Anchor, ContextItem, ContextItemDigests, ContextItemLinks, ConversationScenario,
    EventId, EvidencePointer, FeatureVec, GraphExplain, MessageId, Partition, PlanAction,
    Provenance, SessionId, TenantId,
};
use time::OffsetDateTime;
use uuid::Uuid;

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId(11),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg-e2e".into(),
        config_snapshot_version: 3,
        session_id: Some(SessionId(9)),
        sequence_number: Some(3),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "graph".into(),
            method: "timeline_query".into(),
            model: Some("planner_v1".into()),
            content_digest_sha256: Some("sha256:e2e".into()),
        }),
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    }
}

fn mk_item(anchor: &Anchor, id: &str, partition: Partition, tokens: u32) -> ContextItem {
    ContextItem {
        anchor: anchor.clone(),
        id: id.into(),
        partition,
        partition_hint: Some(partition),
        source_event_id: EventId(100 + u64::from(tokens)),
        source_message_id: Some(MessageId(u64::from(tokens))),
        observed_at: OffsetDateTime::UNIX_EPOCH,
        content: json!({
            "text": format!("{} content", id),
            "kind": match partition {
                Partition::P2Evidence => "tool_result",
                _ => "dialogue",
            },
        }),
        tokens,
        features: FeatureVec {
            rel: 0.7,
            cau: if partition == Partition::P2Evidence {
                0.5
            } else {
                0.6
            },
            rec: 0.6,
            auth: 0.5,
            stab: 0.7,
            dup: 0.2,
            len: 0.4,
            risk: 0.1,
        },
        policy_tags: json!({"stage": id}),
        typ: Some(
            match partition {
                Partition::P2Evidence => "evidence",
                Partition::P3WorkingDelta => "working",
                _ => "dialogue",
            }
            .into(),
        ),
        digests: ContextItemDigests {
            content: Some(format!("sha256:{id}")),
            ..Default::default()
        },
        links: ContextItemLinks {
            evidence_ptrs: vec![EvidencePointer {
                uri: format!("s3://fixtures/{id}.json"),
                digest_sha256: Some("sha256:ok".into()),
                media_type: Some("application/json".into()),
                blob_ref: None,
                span: Some((0, 10)),
                access_policy: Some("restricted".into()),
            }],
            supersedes: None,
        },
    }
}

#[test]
fn e2e_clarify_tool_final_flow() {
    let runtime = LocalContextRuntime::default();

    let mut cfg = ContextConfig::default();
    cfg.snapshot_hash = "cfg-e2e".into();
    cfg.snapshot_version = 3;
    cfg.target_tokens = 260;
    cfg.plan_seed = 7;

    let root_anchor = anchor();
    let clarify = mk_item(&root_anchor, "clarify", Partition::P4Dialogue, 110);
    let tool = mk_item(&root_anchor, "tool_result", Partition::P2Evidence, 180);
    let final_reply = mk_item(&root_anchor, "final", Partition::P4Dialogue, 150);

    let items = vec![clarify, tool, final_reply];
    let output = runtime
        .run(ContextRuntimeInput {
            anchor: root_anchor,
            config: cfg,
            items,
            graph_explain: Some(GraphExplain {
                reasons: vec!["timeline_cache_primary".into(), "recall_topk".into()],
                indices_used: vec!["timeline_v1".into(), "recall_ann_v2".into()],
                query_hash: Some("timeline#9#clarify".into()),
                degradation_reason: Some("clarify_exhausted".into()),
            }),
        })
        .expect("runtime run succeeds");

    let partitions: Vec<Partition> = output
        .bundle
        .segments
        .iter()
        .map(|seg| seg.partition)
        .collect();
    assert!(partitions.contains(&Partition::P2Evidence));
    assert!(partitions.contains(&Partition::P4Dialogue));

    assert!(output
        .bundle
        .explain
        .reasons
        .iter()
        .any(|r| r == "plan_applied"));
    assert!(output
        .bundle
        .explain
        .reasons
        .iter()
        .any(|r| r == "timeline_cache_primary"));
    assert!(output.manifest.manifest_digest.starts_with("man-"));
    assert!(output.env_snapshot.context_digest.starts_with("sha256:"));
    assert!(output
        .bundle
        .explain
        .reasons
        .iter()
        .any(|r| r.contains("graph_degraded:clarify_exhausted")));
    assert_eq!(
        output.bundle.explain.indices_used,
        vec![String::from("timeline_v1"), String::from("recall_ann_v2"),]
    );
    assert_eq!(
        output.bundle.explain.query_hash.as_deref(),
        Some("timeline#9#clarify")
    );

    assert!(output.report.tokens_saved >= 0);
    assert!(output
        .plan
        .items
        .iter()
        .any(|item| item.action != PlanAction::Keep));
}

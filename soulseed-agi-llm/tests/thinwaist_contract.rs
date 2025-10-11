use serde_json::json;
use soulseed_agi_core_models::common::EvidencePointer;
use soulseed_agi_llm::dto::{
    LlmResult, ModelCandidate, ModelProfile, ModelRoutingDecision, PromptBundle, PromptSegment,
    ReasoningVisibility, TokenUsage,
};
use soulseed_agi_llm::tw_client::{MockThinWaistClient, ThinWaistClient};
use soulseed_agi_tools::dto::{Anchor, SessionId, TenantId};
use uuid::Uuid;

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId::new(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg:1".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::new(42)),
        sequence_number: Some(3),
        access_class: soulseed_agi_tools::dto::AccessClass::Internal,
        provenance: None,
        schema_v: 1,
        scenario: None,
        supersedes: None,
        superseded_by: None,
    }
}

#[test]
fn mock_client_contract_roundtrip() {
    let primary = ModelProfile {
        model_id: "llm.standard.v1".into(),
        policy_digest: "sha256:policy".into(),
        safety_tier: "standard".into(),
        max_output_tokens: 2048,
        selection_rank: Some(1),
        selection_score: Some(0.93),
        usage_band: Some("standard".into()),
        estimated_cost_usd: Some(0.0025),
        should_use_factors: json!({"quality": 0.92}),
    };
    let decision = ModelRoutingDecision {
        selected: primary.clone(),
        candidates: vec![ModelCandidate {
            profile: ModelProfile {
                model_id: "llm.backup.v1".into(),
                policy_digest: "sha256:policy-backup".into(),
                safety_tier: "standard".into(),
                max_output_tokens: 1024,
                selection_rank: Some(2),
                selection_score: Some(0.61),
                usage_band: Some("economy".into()),
                estimated_cost_usd: Some(0.0012),
                should_use_factors: json!({"quality": 0.62}),
            },
            exclusion_reason: Some("budget_guard".into()),
            diagnostics: json!({"score": 0.61}),
        }],
        policy_trace: json!({"router_digest": "blake3:router"}),
    };
    let client = MockThinWaistClient::with_decision(decision.clone());

    let selection = client
        .select_model(&anchor(), "demo_scene", &["demo_scene".into()])
        .expect("selection");
    assert_eq!(selection.selected.model_id, "llm.standard.v1");
    assert_eq!(selection.candidates.len(), 1);
    assert_eq!(
        selection.candidates[0].exclusion_reason.as_deref(),
        Some("budget_guard")
    );

    let prompt = PromptBundle {
        system: "sys".into(),
        conversation: vec![PromptSegment {
            role: "user".into(),
            content: "hi".into(),
        }],
        tool_summary: None,
        metadata: json!({"scene": "demo"}),
    };

    let result_payload = LlmResult {
        completion: "ok".into(),
        summary: Some("ok summary".into()),
        evidence_pointer: Some(EvidencePointer {
            uri: "soulbase://llm/result/mock".into(),
            digest_sha256: Some("sha256:mock".into()),
            media_type: Some("text/plain".into()),
            blob_ref: None,
            span: None,
            access_policy: Some("summary_only".into()),
        }),
        provider_metadata: json!({"provider": "mock", "latency_ms": 120}),
        reasoning: Vec::new(),
        reasoning_visibility: ReasoningVisibility::SummaryOnly,
        redacted: false,
        degradation_reason: None,
        indices_used: Some(vec!["idx".into()]),
        query_hash: Some("hash".into()),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_cost_usd: Some(0.0021),
            currency: Some("USD".into()),
        },
        lineage: Default::default(),
    };
    client.push_result(Ok(result_payload.clone()));

    let exec_result = client
        .execute(&anchor(), &prompt, &primary)
        .expect("execute");
    assert_eq!(
        exec_result.evidence_pointer,
        result_payload.evidence_pointer
    );

    client
        .reconcile(&anchor(), &exec_result)
        .expect("reconcile emits payloads");
    let reconciles = client.take_reconciles();
    assert_eq!(reconciles.len(), 3);
    assert!(reconciles[0].get("conversation").is_some());
    assert_eq!(
        reconciles
            .last()
            .unwrap()
            .get("completion")
            .and_then(|v| v.as_str()),
        Some("ok")
    );
}

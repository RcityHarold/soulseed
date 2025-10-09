use httpmock::prelude::*;
use serde_json::json;
use soulseed_agi_core_models::common::EvidencePointer;
use soulseed_agi_llm::dto::{LlmResult, PromptBundle, PromptSegment, ReasoningVisibility, TokenUsage};
use soulseed_agi_llm::tw_client::{SoulbaseThinWaistClient, ThinWaistClient};
use soulseed_agi_tools::dto::{AccessClass, Anchor, ConversationScenario, SessionId, TenantId};
use uuid::Uuid;

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId::new(1),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg:1".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::new(42)),
        sequence_number: Some(3),
        access_class: AccessClass::Internal,
        provenance: None,
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
        supersedes: None,
        superseded_by: None,
    }
}

#[test]
fn soulbase_client_roundtrip() {
    let server = MockServer::start();

    let model = json!({
        "model_id": "llm.standard.v1",
        "policy_digest": "sha256:policy",
        "safety_tier": "standard",
        "max_output_tokens": 2048
    });

    let select_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/llm/select-model")
            .header("content-type", "application/json")
            .header("authorization", "Bearer token")
            .body_contains("\"scene\":\"demo\"")
            .body_contains("\"hints\"");
        then.status(200).json_body(json!({"model": model}));
    });

    let execute_result = json!({
        "result": {
            "completion": "ok",
            "summary": "ok summary",
            "evidence_pointer": {
                "uri": "soulbase://llm/result/mock",
                "digest_sha256": "sha256:mock",
                "media_type": "text/plain"
            },
            "reasoning": [],
            "reasoning_visibility": "summary_only",
            "degradation_reason": null,
            "indices_used": ["idx"],
            "query_hash": "hash",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_cost_usd": 0.0021,
                "currency": "USD"
            }
        }
    });

    let execute_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/llm/execute")
            .header("authorization", "Bearer token")
            .body_contains("\"prompt\"");
        then.status(200).json_body(execute_result.clone());
    });

    let reconcile_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/llm/reconcile")
            .header("authorization", "Bearer token");
        then.status(200).json_body(json!({"status": "ok"}));
    });

    let client = SoulbaseThinWaistClient::new(server.base_url(), "token").expect("client");
    let anchor = anchor();
    let model_profile = client
        .select_model(&anchor, "demo", &["demo".into()])
        .expect("select");
    assert_eq!(model_profile.model_id, "llm.standard.v1");
    select_mock.assert();

    let prompt = PromptBundle {
        system: "sys".into(),
        conversation: vec![PromptSegment {
            role: "user".into(),
            content: "hi".into(),
        }],
        tool_summary: None,
        metadata: json!({}),
    };
    let result = client
        .execute(&anchor, &prompt, &model_profile)
        .expect("execute");
    assert_eq!(result.completion, "ok");
    assert_eq!(result.summary.as_deref(), Some("ok summary"));
    assert_eq!(
        result
            .evidence_pointer
            .as_ref()
            .expect("evidence pointer")
            .uri,
        "soulbase://llm/result/mock"
    );
    execute_mock.assert();

    client
        .reconcile(
            &anchor,
            &LlmResult {
                completion: "ok".into(),
                summary: Some("ok summary".into()),
                evidence_pointer: Some(EvidencePointer {
                    uri: "soulbase://llm/result/mock-local".into(),
                    digest_sha256: Some("sha256:mock-local".into()),
                    media_type: Some("text/plain".into()),
                    blob_ref: None,
                    span: None,
                    access_policy: Some("summary_only".into()),
                }),
                reasoning: Vec::new(),
                reasoning_visibility: ReasoningVisibility::SummaryOnly,
                degradation_reason: None,
                indices_used: None,
                query_hash: None,
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    ..Default::default()
                },
            },
        )
        .expect("reconcile");
    reconcile_mock.assert();
}

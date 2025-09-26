use httpmock::prelude::*;
use serde_json::json;
use soulseed_agi_tools::dto::{
    AccessClass, Anchor, ConversationScenario, SessionId, TenantId, ToolCallSpec,
};
use soulseed_agi_tools::tw_client::{
    SoulbaseToolsClient, Subject, ThinWaistClient, ToolDef, TwEvent,
};
use uuid::Uuid;

fn anchor() -> Anchor {
    Anchor {
        tenant_id: TenantId::new(7),
        envelope_id: Uuid::now_v7(),
        config_snapshot_hash: "cfg:tool".into(),
        config_snapshot_version: 2,
        session_id: Some(SessionId::new(55)),
        sequence_number: Some(9),
        access_class: AccessClass::Internal,
        provenance: None,
        schema_v: 1,
        scenario: Some(ConversationScenario::HumanToAi),
    }
}

fn subject() -> Subject {
    Subject {
        human_id: Some(soulseed_agi_tools::dto::HumanId::new(123)),
        ai_id: None,
    }
}

#[test]
fn soulbase_tools_client_http_roundtrip() {
    let server = MockServer::start();

    let tools = vec![ToolDef {
        tool_id: "web.search".into(),
        version: "1".into(),
        capability: vec!["search".into()],
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        side_effect: false,
        supports_stream: true,
        risk_level: "low".into(),
    }];

    let list_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/tools/list")
            .header("authorization", "Bearer token")
            .body_contains("\"scene\":\"demo\"")
            .body_contains("\"capabilities\"");
        then.status(200)
            .json_body(json!({"tools": tools, "policy_digest": "sha256:policy"}));
    });

    let precharge_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/tools/precharge")
            .header("authorization", "Bearer token")
            .body_contains("\"tool_id\":\"web.search\"")
            .body_contains("\"idem_key\":\"idem-1\"");
        then.status(200)
            .json_body(json!({"decision": "Allow", "reservation": null}));
    });

    let execute_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/tools/execute")
            .header("authorization", "Bearer token")
            .body_contains("\"tool_id\":\"web.search\"")
            .body_contains("\"call\"");
        then.status(200).json_body(json!({
            "result": {
                "summary": {
                    "tool_id": "web.search",
                    "schema_v": 1,
                    "summary": {"hits": 3},
                    "evidence_pointer": {
                        "uri": "s3://bucket",
                        "blob_ref": null,
                        "span": null,
                        "checksum": "sha256:sum",
                        "access_policy": "internal"
                    },
                    "result_digest": "digest"
                },
                "degradation_reason": "timeout",
                "indices_used": ["idx1"],
                "query_hash": "hash1"
            }
        }));
    });

    let reconcile_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/tools/reconcile")
            .header("authorization", "Bearer token")
            .body_contains("\"idem_key\":\"idem-1\"")
            .body_contains("\"status\":\"ok\"");
        then.status(200).json_body(json!({"status": "ok"}));
    });

    let events_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/tools/emit-events")
            .header("authorization", "Bearer token")
            .body_contains("ToolResponded");
        then.status(200).json_body(json!({"accepted": true}));
    });

    let client = SoulbaseToolsClient::new(server.base_url(), "token").expect("client");
    let anchor = anchor();
    let capabilities = vec!["search".into()];

    let (listed, policy) = client
        .tools_list(&anchor, "demo", &capabilities)
        .expect("tools_list");
    assert_eq!(listed.len(), 1);
    assert_eq!(policy.as_deref(), Some("sha256:policy"));
    list_mock.assert();

    let subject = subject();
    let decision = client
        .precharge(&anchor, &subject, "web.search", "idem-1")
        .expect("precharge");
    assert_eq!(decision.decision, "Allow");
    precharge_mock.assert();

    let call = ToolCallSpec {
        tool_id: "web.search".into(),
        schema_v: 1,
        input: json!({"query": "Soulseed"}),
        params: json!({}),
        cacheable: false,
        stream: false,
        idem_key: "idem-1".into(),
    };

    let exec = client.execute(&anchor, &subject, &call).expect("execute");
    assert_eq!(exec.summary.tool_id, "web.search");
    assert_eq!(exec.degradation_reason.as_deref(), Some("timeout"));
    execute_mock.assert();

    client
        .reconcile(&anchor, "idem-1", &json!({"status": "ok"}))
        .expect("reconcile");
    reconcile_mock.assert();

    client
        .emit_events(&[TwEvent::ToolResponded {
            tool_id: "web.search".into(),
            idem_key: "idem-1".into(),
        }])
        .expect("emit events");
    events_mock.assert();
}

use soulseed_agi_core_models::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event = DialogueEvent {
        tenant_id: TenantId::new(42),
        event_id: EventId::new(1),
        session_id: SessionId::new(100),
        subject: Subject::Human(HumanId::new(7)),
        participants: vec![SubjectRef {
            kind: Subject::AI(AIId::new(8)),
            role: Some("companion".into()),
        }],
        head: EnvelopeHead {
            envelope_id: new_envelope_id(),
            trace_id: TraceId("trc-1".into()),
            correlation_id: CorrelationId("corr-1".into()),
            config_snapshot_hash: "cfg-hash".into(),
            config_snapshot_version: 1,
        },
        snapshot: Snapshot {
            schema_v: 1,
            created_at: time::OffsetDateTime::now_utc(),
        },
        timestamp_ms: 1_700_000_000_000,
        scenario: ConversationScenario::HumanToAi,
        event_type: DialogueEventType::Message,
        time_window: Some("2025-01-01T00".into()),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "chat-ui".into(),
            method: "llm".into(),
            model: Some("mock-model".into()),
            content_digest_sha256: Some("sha256:demo".into()),
        }),
        sequence_number: 1,
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: vec![],
        reasoning_trace: Some("internal reasoning redacted".into()),
        reasoning_confidence: Some(0.8),
        reasoning_strategy: Some("self-reflection".into()),
        content_embedding: Some(vec![0.1; 4]),
        context_embedding: Some(vec![0.2; 4]),
        decision_embedding: Some(vec![0.3; 4]),
        embedding_meta: Some(EmbeddingMeta {
            model: "mock-emb".into(),
            dim: 4,
            ts: 1_700_000_000_000,
        }),
        concept_vector: None,
        semantic_cluster_id: Some("topic:travel".into()),
        cluster_method: Some("hnsw".into()),
        concept_distance_to_goal: Some(0.12),
        real_time_priority: Some(RealTimePriority::Normal),
        notification_targets: None,
        live_stream_id: None,
        growth_stage: Some("ic:responding".into()),
        processing_latency_ms: Some(42),
        influence_score: Some(0.67),
        community_impact: None,
        evidence_pointer: Some(EvidencePointer {
            uri: "blob://evidence/ev-1".into(),
            digest_sha256: Some("sha256:payload".into()),
            media_type: Some("application/json".into()),
            blob_ref: None,
            span: None,
            access_policy: Some("internal".into()),
        }),
        content_digest_sha256: Some("sha256:payload".into()),
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(MessagePointer {
            message_id: MessageId::new(555),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: serde_json::json!({"scenario": "Consultation"}),
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    };

    event.validate()?;
    println!("DialogueEvent passed validation");
    Ok(())
}

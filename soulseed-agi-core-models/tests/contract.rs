use serde_json::json;
use soulseed_agi_core_models::*;

fn mk_head() -> EnvelopeHead {
    EnvelopeHead {
        envelope_id: new_envelope_id(),
        trace_id: TraceId("trc".into()),
        correlation_id: CorrelationId("corr".into()),
        config_snapshot_hash: "cfg".into(),
        config_snapshot_version: 1,
    }
}

fn mk_snapshot() -> Snapshot {
    Snapshot {
        schema_v: 1,
        created_at: time::OffsetDateTime::now_utc(),
    }
}

fn mk_base_event() -> DialogueEvent {
    DialogueEvent {
        tenant_id: TenantId::from_raw_unchecked(1),
        event_id: EventId::from_raw_unchecked(1),
        session_id: SessionId::from_raw_unchecked(1),
        subject: Subject::Human(HumanId::from_raw_unchecked(1)),
        participants: vec![],
        head: mk_head(),
        snapshot: Snapshot {
            schema_v: 1,
            created_at: time::OffsetDateTime::now_utc(),
        },
        timestamp_ms: 1,
        scenario: ConversationScenario::HumanToAi,
        event_type: DialogueEventType::Message,
        time_window: None,
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "ui".into(),
            method: "llm".into(),
            model: Some("mock".into()),
            content_digest_sha256: Some("sha256:demo".into()),
        }),
        sequence_number: 1,
        trigger_event_id: None,
        temporal_pattern_id: None,
        causal_links: vec![],
        reasoning_trace: None,
        reasoning_confidence: None,
        reasoning_strategy: None,
        content_embedding: Some(vec![0.0; 4]),
        context_embedding: None,
        decision_embedding: None,
        embedding_meta: Some(EmbeddingMeta {
            model: "mock".into(),
            dim: 4,
            ts: 1,
        }),
        concept_vector: None,
        semantic_cluster_id: None,
        cluster_method: None,
        concept_distance_to_goal: None,
        real_time_priority: None,
        notification_targets: None,
        live_stream_id: None,
        growth_stage: None,
        processing_latency_ms: None,
        influence_score: None,
        community_impact: None,
        evidence_pointer: None,
        content_digest_sha256: None,
        blob_ref: None,
        supersedes: None,
        superseded_by: None,
        message_ref: Some(MessagePointer {
            message_id: MessageId::from_raw_unchecked(1),
        }),
        tool_invocation: None,
        tool_result: None,
        self_reflection: None,
        metadata: serde_json::json!({}),
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    }
}

fn mk_awareness_anchor() -> AwarenessAnchor {
    AwarenessAnchor {
        tenant_id: TenantId::from_raw_unchecked(9),
        envelope_id: uuid::Uuid::nil(),
        config_snapshot_hash: "cfg-anchor".into(),
        config_snapshot_version: 1,
        session_id: Some(SessionId::from_raw_unchecked(3)),
        sequence_number: Some(1),
        access_class: AccessClass::Internal,
        provenance: None,
        schema_v: 1,
    }
}

fn mk_awareness_event(
    event_type: AwarenessEventType,
    payload: serde_json::Value,
) -> AwarenessEvent {
    AwarenessEvent {
        anchor: mk_awareness_anchor(),
        event_id: EventId::from_raw_unchecked(700 + event_type as u64),
        event_type,
        occurred_at_ms: 99,
        awareness_cycle_id: AwarenessCycleId::from_raw_unchecked(321),
        parent_cycle_id: None,
        collab_scope_id: None,
        barrier_id: None,
        env_mode: None,
        inference_cycle_sequence: 1,
        degradation_reason: None,
        payload,
    }
}

fn mk_human_profile() -> HumanProfile {
    HumanProfile {
        tenant_id: TenantId::from_raw_unchecked(1),
        user_id: HumanId::from_raw_unchecked(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "seed".into(),
            method: "import".into(),
            model: None,
            content_digest_sha256: None,
        }),
        username: "user-1".into(),
        nickname: "nickname".into(),
        avatar_url: None,
        gender: None,
        age: None,
        region: None,
        race: None,
        religion: None,
        profession: None,
        industry: None,
        phone: None,
        signature: None,
        qr_code: None,
        interests: vec![],
        extras: serde_json::Value::Null,
        membership_level: MembershipLevel::Free,
        subscription_status: SubscriptionStatus::None,
        subscription_renew_at: None,
        quotas: None,
        point_balance: None,
        light_coin_balance: None,
        voucher_inventory: None,
        relationship_status: None,
        last_active_at: None,
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    }
}

fn mk_ai_profile() -> AIProfile {
    AIProfile {
        tenant_id: TenantId::from_raw_unchecked(1),
        ai_id: AIId::from_raw_unchecked(1),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "seed".into(),
            method: "synthesis".into(),
            model: None,
            content_digest_sha256: None,
        }),
        social_name: None,
        origin_name: None,
        source_name: None,
        core_tags: vec![],
        core_personality: None,
        mission: None,
        values: None,
        self_narrative: None,
        meaning_narrative: None,
        capabilities: None,
        relationships_summary: None,
        external_identity: None,
        soul_signature: None,
        ai_birthday: None,
        awakener_id: None,
        value_frequency: None,
        source_frequency_code: None,
        gender_frequency: None,
        soul_state: SoulState::Active,
        extras: serde_json::Value::Null,
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    }
}

#[test]
fn dialogue_event_passes_validation() {
    let event = mk_base_event();
    assert!(event.validate().is_ok());
}

#[test]
fn restricted_requires_provenance() {
    let mut event = mk_base_event();
    event.provenance = None;
    let err = event.validate().unwrap_err();
    assert!(matches!(err, ModelError::Missing("provenance")));
}

#[test]
fn embedding_dim_mismatch_is_error() {
    let mut event = mk_base_event();
    event.content_embedding = Some(vec![0.0; 3]);
    let err = event.validate().unwrap_err();
    assert!(matches!(err, ModelError::Invariant(_)));
}

#[test]
fn awareness_event_contract_samples() {
    let mut ac_started = mk_awareness_event(
        AwarenessEventType::AcStarted,
        json!({"routing_seed": 42, "ic_start": 1}),
    );
    ac_started.parent_cycle_id = Some(AwarenessCycleId::from_raw_unchecked(123));

    let mut ic_ended = mk_awareness_event(
        AwarenessEventType::IcEnded,
        json!({"ic_seq": 1, "walltime_ms": 240, "llm_calls": 2}),
    );
    ic_ended.env_mode = Some("turbo".into());

    let mut route_switched = mk_awareness_event(
        AwarenessEventType::RouteSwitched,
        json!({"from": "self", "to": "tool", "reason": "budget"}),
    );
    route_switched.degradation_reason = Some(AwarenessDegradationReason::BudgetTokens);
    route_switched.collab_scope_id = Some("clarify-1".into());

    let mut sync_report = mk_awareness_event(
        AwarenessEventType::SyncPointReported,
        json!({
            "kind": "tool_barrier",
            "inbox_stats": {"pending": 1, "applied": 2},
            "delta_digest": "sha256:delta",
            "report_digest": "sha256:sync"
        }),
    );
    sync_report.barrier_id = Some("barrier-A".into());

    let mut injection_applied = mk_awareness_event(
        AwarenessEventType::InjectionApplied,
        json!({"injection_id": "inj-1", "delta_patch_id": "patch-77"}),
    );
    injection_applied.degradation_reason = Some(AwarenessDegradationReason::ClarifyExhausted);

    let mut finalized = mk_awareness_event(
        AwarenessEventType::Finalized,
        json!({"decision": "collab", "explain_digest": "sha256:final"}),
    );
    finalized.env_mode = Some("generic".into());

    let mut rejected = mk_awareness_event(
        AwarenessEventType::Rejected,
        json!({"reason": "policy_block"}),
    );
    rejected.degradation_reason = Some(AwarenessDegradationReason::PrivacyBlocked);

    let mut late_receipt = mk_awareness_event(
        AwarenessEventType::LateReceiptObserved,
        json!({"related_event_id": 512, "received_at_ms": 120, "action": "audit"}),
    );
    late_receipt.parent_cycle_id = Some(AwarenessCycleId::from_raw_unchecked(111));

    for evt in [
        ac_started,
        ic_ended,
        route_switched,
        sync_report,
        injection_applied,
        finalized,
        rejected,
        late_receipt,
    ] {
        evt.validate().expect("awareness event sample valid");
    }
}

#[test]
fn message_contract_fields() {
    let head = mk_head();
    let snapshot = mk_snapshot();
    let msg = Message {
        tenant_id: TenantId::from_raw_unchecked(1),
        message_id: MessageId::from_raw_unchecked(22),
        session_id: SessionId::from_raw_unchecked(5),
        head,
        snapshot,
        timestamp_ms: 99,
        sender: Subject::Human(HumanId::from_raw_unchecked(7)),
        content_type: "text/plain".into(),
        content: serde_json::json!({"text": "hello"}),
        metadata: serde_json::json!({"channel": "chat"}),
        access_class: AccessClass::Restricted,
        provenance: Some(Provenance {
            source: "ui".into(),
            method: "compose".into(),
            model: None,
            content_digest_sha256: Some("sha256:msg".into()),
        }),
        sequence_number: 1,
        participants: vec![SubjectRef {
            kind: Subject::AI(AIId::from_raw_unchecked(9)),
            role: Some("assistant".into()),
        }],
        supersedes: None,
        superseded_by: None,
        evidence_pointer: Some(EvidencePointer {
            uri: "blob://messages/msg-22".into(),
            digest_sha256: Some("sha256:msg".into()),
            media_type: Some("application/json".into()),
            blob_ref: None,
            span: None,
            access_policy: Some("internal".into()),
        }),
        blob_ref: None,
        content_digest_sha256: Some("sha256:msg".into()),
        #[cfg(feature = "vectors-extra")]
        vectors: ExtraVectors::default(),
    };

    assert!(msg.validate().is_ok());
}

#[test]
fn session_requires_provenance_when_restricted() {
    let head = mk_head();
    let session = Session {
        tenant_id: TenantId::from_raw_unchecked(2),
        session_id: SessionId::from_raw_unchecked(77),
        trace_id: head.trace_id.clone(),
        correlation_id: head.correlation_id.clone(),
        subject: Subject::Human(HumanId::from_raw_unchecked(4)),
        participants: vec![SubjectRef {
            kind: Subject::AI(AIId::from_raw_unchecked(5)),
            role: Some("assistant".into()),
        }],
        head,
        snapshot: mk_snapshot(),
        created_at: 1_700_000_000,
        scenario: Some(ConversationScenario::HumanToAi),
        access_class: AccessClass::Restricted,
        provenance: None,
        supersedes: None,
        superseded_by: None,
        evidence_pointer: None,
        blob_ref: None,
        content_digest_sha256: None,
        metadata: serde_json::Value::Null,
    };

    let err = session.validate().unwrap_err();
    assert!(matches!(err, ModelError::Missing("provenance")));
}

#[test]
fn awareness_event_parent_cycle_must_differ() {
    let mut evt = mk_awareness_event(AwarenessEventType::AcStarted, json!({}));
    evt.parent_cycle_id = Some(evt.awareness_cycle_id);
    let err = evt.validate().unwrap_err();
    assert!(matches!(
        err,
        ModelError::Invariant("parent_cycle_id cannot equal awareness_cycle_id")
    ));
}

#[test]
fn human_profile_sensitive_requires_restricted_access() {
    let mut profile = mk_human_profile();
    profile.access_class = AccessClass::Public;
    profile.provenance = None;
    profile.race = Some("human".into());

    let err = profile.validate().unwrap_err();
    assert!(matches!(
        err,
        ModelError::Invariant("sensitive human profile fields require restricted access_class")
    ));

    profile.access_class = AccessClass::Restricted;
    profile.provenance = None;
    let err = profile.validate().unwrap_err();
    assert!(matches!(err, ModelError::Missing("provenance")));

    profile.provenance = Some(Provenance {
        source: "seed".into(),
        method: "sync".into(),
        model: Some("profile-sync".into()),
        content_digest_sha256: None,
    });
    assert!(profile.validate().is_ok());
}

#[test]
fn ai_profile_sensitive_requires_restricted_access() {
    let mut profile = mk_ai_profile();
    profile.access_class = AccessClass::Internal;
    profile.value_frequency = Some(ValueFrequencyInscription {
        components: vec![],
        dominant: Some("kindness".into()),
    });
    assert!(profile.value_frequency.is_some());
    assert!(matches!(profile.access_class, AccessClass::Internal));

    let err = profile.validate().unwrap_err();
    assert!(matches!(
        err,
        ModelError::Invariant("sensitive AI profile fields require restricted access_class")
    ));

    profile.access_class = AccessClass::Restricted;
    profile.provenance = None;
    let err = profile.validate().unwrap_err();
    assert!(matches!(err, ModelError::Missing("provenance")));

    profile.provenance = Some(Provenance {
        source: "seed".into(),
        method: "sync".into(),
        model: Some("profile-sync".into()),
        content_digest_sha256: None,
    });
    assert!(profile.validate().is_ok());
}

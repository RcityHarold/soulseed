use serde_json::{Value, json};
use std::sync::Arc;

use crate::ca::{CaService, CaServiceNoop, InjectionAction, MergeDeltaRequest, MergeDeltaResponse};
use crate::errors::AceError;
use crate::types::{AggregationOutcome, SyncPointInput, SyncPointReport};
use soulseed_agi_core_models::awareness::{
    AwarenessDegradationReason, AwarenessEvent, AwarenessEventType,
};

pub struct SyncPointAggregator {
    ca: Arc<dyn CaService>,
}

impl Default for SyncPointAggregator {
    fn default() -> Self {
        let ca: Arc<dyn CaService> = Arc::new(CaServiceNoop::default());
        Self { ca }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hitl::{HitlInjection, HitlPriority};
    use crate::types::BudgetSnapshot;
    use serde_json::json;
    use soulseed_agi_core_models::awareness::{AwarenessAnchor, AwarenessEventType, SyncPointKind};
    use soulseed_agi_core_models::{
        AccessClass, ConversationScenario, CorrelationId, DialogueEvent, DialogueEventType,
        EnvelopeHead, EventId, Snapshot, Subject, SubjectRef, TenantId, TraceId,
    };
    use std::collections::HashMap;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    #[derive(Clone)]
    struct StubCa {
        response: crate::ca::MergeDeltaResponse,
    }

    impl CaService for StubCa {
        fn merge_delta(
            &self,
            _request: MergeDeltaRequest,
        ) -> Result<crate::ca::MergeDeltaResponse, AceError> {
            Ok(self.response.clone())
        }
    }

    fn anchor() -> AwarenessAnchor {
        AwarenessAnchor {
            tenant_id: TenantId::new(7),
            envelope_id: Uuid::now_v7(),
            config_snapshot_hash: "cfg".into(),
            config_snapshot_version: 1,
            session_id: Some(soulseed_agi_core_models::SessionId::new(5)),
            sequence_number: Some(3),
            access_class: AccessClass::Internal,
            provenance: None,
            schema_v: 1,
        }
    }

    fn dialogue_event(anchor: &AwarenessAnchor) -> DialogueEvent {
        let now = OffsetDateTime::now_utc();
        DialogueEvent {
            tenant_id: anchor.tenant_id,
            event_id: EventId(now.unix_timestamp_nanos() as u64),
            session_id: anchor.session_id.unwrap(),
            subject: Subject::AI(soulseed_agi_core_models::AIId::new(9)),
            participants: vec![SubjectRef {
                kind: Subject::Human(soulseed_agi_core_models::HumanId::new(1)),
                role: Some("user".into()),
            }],
            head: EnvelopeHead {
                envelope_id: anchor.envelope_id,
                trace_id: TraceId("trace".into()),
                correlation_id: CorrelationId("corr".into()),
                config_snapshot_hash: anchor.config_snapshot_hash.clone(),
                config_snapshot_version: anchor.config_snapshot_version,
            },
            snapshot: Snapshot {
                schema_v: anchor.schema_v,
                created_at: now,
            },
            timestamp_ms: now.unix_timestamp() * 1000,
            scenario: ConversationScenario::HumanToAi,
            event_type: DialogueEventType::Message,
            time_window: None,
            access_class: anchor.access_class,
            provenance: anchor.provenance.clone(),
            sequence_number: anchor.sequence_number.unwrap(),
            trigger_event_id: None,
            temporal_pattern_id: None,
            reasoning_trace: None,
            reasoning_confidence: None,
            reasoning_strategy: None,
            content_embedding: None,
            context_embedding: None,
            decision_embedding: None,
            embedding_meta: None,
            concept_vector: None,
            semantic_cluster_id: None,
            cluster_method: None,
            concept_distance_to_goal: None,
            real_time_priority: None,
            notification_targets: None,
            live_stream_id: None,
            evidence_pointer: None,
            content_digest_sha256: None,
            blob_ref: None,
            supersedes: None,
            superseded_by: None,
            message_ref: Some(soulseed_agi_core_models::MessagePointer {
                message_id: soulseed_agi_core_models::MessageId::new(1),
            }),
            tool_invocation: None,
            tool_result: None,
            self_reflection: None,
            metadata: json!({"degradation_reason": "clarify_concurrency"}),
        }
    }

    #[test]
    fn aggregates_ca_output_into_report() {
        let anchor = anchor();
        let now = OffsetDateTime::now_utc();
        let delta_patch = soulseed_agi_core_models::awareness::DeltaPatch {
            patch_id: "patch-1".into(),
            added: vec!["ctx-1".into()],
            updated: Vec::new(),
            removed: Vec::new(),
            score_stats: HashMap::new(),
            why_included: None,
            pointers: None,
            patch_digest: "digest-123".into(),
        };

        let response = crate::ca::MergeDeltaResponse {
            delta_patch: Some(delta_patch.clone()),
            awareness_events: Vec::new(),
            injections: vec![crate::ca::InjectionDecision {
                injection_id: Uuid::now_v7(),
                action: InjectionAction::Applied,
                reason: Some("hitl_priority".into()),
                fingerprint: Some("fp-1".into()),
            }],
        };

        let ca: Arc<dyn CaService> = Arc::new(StubCa { response });
        let aggregator = SyncPointAggregator::new(ca);

        let budget = BudgetSnapshot {
            tokens_allowed: 400,
            tokens_spent: 120,
            walltime_ms_allowed: 5_000,
            walltime_ms_used: 900,
        };

        let injection = HitlInjection::new(
            anchor.tenant_id,
            HitlPriority::P1High,
            "system",
            json!({
                "msg": "clarify asap"
            }),
        );

        let input = SyncPointInput {
            cycle_id: soulseed_agi_core_models::CycleId(88),
            kind: SyncPointKind::ClarifyAnswered,
            anchor: anchor.clone(),
            events: vec![dialogue_event(&anchor)],
            budget: budget.clone(),
            timeframe: (now - Duration::seconds(2), now),
            pending_injections: vec![injection],
            context_manifest: json!({"entries": [1, 2, 3]}),
        };

        let outcome = aggregator.aggregate(input).expect("aggregate");
        assert_eq!(outcome.report.kind, SyncPointKind::ClarifyAnswered);
        assert_eq!(outcome.report.metrics["injections_applied"], 1);
        assert_eq!(outcome.report.metrics["manifest_entries"], 3);
        assert_eq!(
            outcome.report.delta_patch_digest.as_deref(),
            Some("digest-123")
        );
        assert_eq!(outcome.delta_patch.unwrap().patch_digest, "digest-123");
        assert_eq!(outcome.injections.len(), 1);
        assert_eq!(outcome.awareness_events.len(), 2);
        let injection_event = outcome
            .awareness_events
            .iter()
            .find(|event| event.event_type == AwarenessEventType::InjectionApplied)
            .expect("injection applied event");
        assert_eq!(injection_event.payload["reason"], "hitl_priority");

        let summary_event = outcome
            .awareness_events
            .iter()
            .find(|event| event.event_type == AwarenessEventType::SyncPointReported)
            .expect("summary event");
        assert_eq!(
            summary_event.payload["degradation_reason"],
            json!("clarify_concurrency")
        );
        assert_eq!(
            outcome.report.degradation_reason.as_deref(),
            Some("clarify_concurrency")
        );
    }
}

impl SyncPointAggregator {
    pub fn new(ca: Arc<dyn CaService>) -> Self {
        Self { ca }
    }

    pub fn aggregate(&self, input: SyncPointInput) -> Result<AggregationOutcome, AceError> {
        if input.events.is_empty() {
            return Err(AceError::InvalidRequest(
                "sync point requires events".into(),
            ));
        }

        let request = MergeDeltaRequest::from(&input);
        let response = self.ca.merge_delta(request)?;
        let MergeDeltaResponse {
            delta_patch,
            awareness_events,
            injections,
        } = response;

        let summary = format!(
            "syncpoint:{:?} events={} budget_spent={}",
            input.kind,
            input.events.len(),
            input.budget.tokens_spent
        );

        let degradation_reason = input
            .events
            .iter()
            .find_map(|ev| {
                ev.metadata
                    .get("degradation_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                injections
                    .iter()
                    .find_map(|decision| decision.reason.clone())
            });

        let manifest_entries = input
            .context_manifest
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or_default();

        let (applied, deferred, ignored) =
            injections
                .iter()
                .fold((0u32, 0u32, 0u32), |mut acc, decision| {
                    match decision.action {
                        InjectionAction::Applied => acc.0 += 1,
                        InjectionAction::Deferred => acc.1 += 1,
                        InjectionAction::Ignored => acc.2 += 1,
                    }
                    acc
                });

        let window_ms = (input.timeframe.1 - input.timeframe.0).whole_milliseconds();
        let metrics = json!({
            "events": input.events.len(),
            "tokens_spent": input.budget.tokens_spent,
            "window_ms": window_ms,
            "pending_injections": input.pending_injections.len(),
            "injections_applied": applied,
            "injections_deferred": deferred,
            "injections_ignored": ignored,
            "manifest_entries": manifest_entries,
        });

        let mut awareness_events = awareness_events;
        let mut next_event_id = awareness_events
            .iter()
            .map(|event| event.event_id.0)
            .max()
            .unwrap_or(input.cycle_id.0)
            + 1;
        let occurred_at_ms = input.timeframe.1.unix_timestamp() * 1000;

        for decision in &injections {
            let event_type = match decision.action {
                InjectionAction::Applied => AwarenessEventType::InjectionApplied,
                InjectionAction::Deferred => AwarenessEventType::InjectionDeferred,
                InjectionAction::Ignored => AwarenessEventType::InjectionIgnored,
            };

            let payload = json!({
                "injection_id": decision.injection_id,
                "action": format!("{:?}", decision.action),
                "reason": decision.reason,
                "fingerprint": decision.fingerprint,
            });

            awareness_events.push(AwarenessEvent {
                anchor: input.anchor.clone(),
                event_id: soulseed_agi_core_models::EventId(next_event_id),
                event_type,
                occurred_at_ms,
                awareness_cycle_id: input.cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: map_degradation(decision.reason.as_deref()),
                payload,
            });
            next_event_id += 1;
        }

        let mut summary_payload = json!({
            "summary": summary,
            "metrics": metrics.clone(),
        });
        if let (Some(reason), Value::Object(map)) = (&degradation_reason, &mut summary_payload) {
            map.insert("degradation_reason".into(), json!(reason));
        }

        awareness_events.push(AwarenessEvent {
            anchor: input.anchor.clone(),
            event_id: soulseed_agi_core_models::EventId(next_event_id),
            event_type: AwarenessEventType::SyncPointReported,
            occurred_at_ms,
            awareness_cycle_id: input.cycle_id,
            parent_cycle_id: None,
            collab_scope_id: None,
            barrier_id: None,
            env_mode: None,
            inference_cycle_sequence: 1,
            degradation_reason: map_degradation(degradation_reason.as_deref()),
            payload: summary_payload,
        });

        let delta_digest = delta_patch.as_ref().map(|patch| patch.patch_digest.clone());

        let report = SyncPointReport {
            cycle_id: input.cycle_id,
            kind: input.kind,
            summary,
            degradation_reason,
            metrics: metrics.clone(),
            injections: injections.clone(),
            delta_patch_digest: delta_digest.clone(),
        };

        Ok(AggregationOutcome {
            report,
            awareness_events,
            delta_patch,
            injections,
        })
    }
}

fn map_degradation(reason: Option<&str>) -> Option<AwarenessDegradationReason> {
    match reason.unwrap_or_default() {
        "clarify_high_priority"
        | "clarify_conflict"
        | "timeout_fallback"
        | "llm_timeout_recovered"
        | "requires_follow_up" => Some(AwarenessDegradationReason::ClarifyExhausted),
        "stale_fact" => Some(AwarenessDegradationReason::GraphDegraded),
        "timeout" => Some(AwarenessDegradationReason::BudgetWalltime),
        "budget_tokens" => Some(AwarenessDegradationReason::BudgetTokens),
        "budget_walltime" => Some(AwarenessDegradationReason::BudgetWalltime),
        "budget_external_cost" => Some(AwarenessDegradationReason::BudgetExternalCost),
        "graph_degraded" => Some(AwarenessDegradationReason::GraphDegraded),
        "envctx_degraded" => Some(AwarenessDegradationReason::EnvctxDegraded),
        "privacy_blocked" => Some(AwarenessDegradationReason::PrivacyBlocked),
        "invalid_plan" => Some(AwarenessDegradationReason::InvalidPlan),
        "empty_catalog" => Some(AwarenessDegradationReason::EmptyCatalog),
        _ => None,
    }
}

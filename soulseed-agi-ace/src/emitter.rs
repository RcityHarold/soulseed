use serde_json::{Value, json};

use crate::errors::AceError;
use crate::types::{CycleEmission, OutboxEnvelope, OutboxMessage};
use soulseed_agi_core_models::awareness::{AwarenessEvent, AwarenessEventType};

pub struct Emitter;

impl Emitter {
    pub fn emit(&self, emission: CycleEmission) -> Result<OutboxEnvelope, AceError> {
        let parent_cycle_id = emission.parent_cycle_id;
        let collab_scope = emission.collab_scope_id.clone();
        let mut messages: Vec<OutboxMessage> = emission
            .awareness_events
            .into_iter()
            .map(|mut payload| {
                if payload.parent_cycle_id.is_none() {
                    payload.parent_cycle_id = parent_cycle_id;
                }
                if payload.collab_scope_id.is_none() {
                    if let Some(scope) = &collab_scope {
                        payload.collab_scope_id = Some(scope.clone());
                    }
                }
                OutboxMessage {
                    cycle_id: emission.cycle_id,
                    event_id: payload.event_id,
                    payload,
                }
            })
            .collect();

        let mut final_payload = json!({
            "final_event_id": emission.final_event.base.event_id.as_u64(),
            "lane": format!("{:?}", emission.lane),
        });
        if let Value::Object(map) = &mut final_payload {
            if let Some(digest) = &emission.manifest_digest {
                map.insert("manifest_digest".into(), json!(digest));
            }
            if let Some(scope) = &emission.collab_scope_id {
                map.insert("collab_scope_id".into(), json!(scope));
            }
            if let Some(fingerprint) = &emission.explain_fingerprint {
                map.insert("explain_fingerprint".into(), json!(fingerprint));
            }
            map.insert(
                "budget_snapshot".into(),
                json!({
                    "tokens_allowed": emission.budget.tokens_allowed,
                    "tokens_spent": emission.budget.tokens_spent,
                    "walltime_ms_allowed": emission.budget.walltime_ms_allowed,
                    "walltime_ms_used": emission.budget.walltime_ms_used,
                    "external_cost_allowed": emission.budget.external_cost_allowed,
                    "external_cost_spent": emission.budget.external_cost_spent,
                }),
            );
            map.insert(
                "router_digest".into(),
                json!(emission.router_decision.plan.explain.router_digest.clone()),
            );
        }

        messages.push(OutboxMessage {
            cycle_id: emission.cycle_id,
            event_id: emission.final_event.base.event_id,
            payload: AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: emission.final_event.base.event_id,
                event_type: AwarenessEventType::Finalized,
                occurred_at_ms: emission.final_event.base.timestamp_ms,
                awareness_cycle_id: emission.cycle_id,
                parent_cycle_id,
                collab_scope_id: emission.collab_scope_id.clone(),
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: final_payload,
            },
        });

        let mut ended_payload = json!({
            "lane": format!("{:?}", emission.lane),
            "status": format!("{:?}", emission.status),
            "final_event_id": emission.final_event.base.event_id.as_u64(),
            "router_digest": emission.router_decision.plan.explain.router_digest.clone(),
        });
        if let Value::Object(map) = &mut ended_payload {
            if let Some(digest) = &emission.manifest_digest {
                map.insert("manifest_digest".into(), json!(digest));
            }
            if let Some(scope) = &emission.collab_scope_id {
                map.insert("collab_scope_id".into(), json!(scope));
            }
            if let Some(fingerprint) = &emission.explain_fingerprint {
                map.insert("explain_fingerprint".into(), json!(fingerprint));
            }
            if let Some(parent) = parent_cycle_id {
                map.insert("parent_cycle_id".into(), json!(parent.as_u64()));
            }
        }
        let ended_event_id = soulseed_agi_core_models::EventId::generate();
        messages.push(OutboxMessage {
            cycle_id: emission.cycle_id,
            event_id: ended_event_id,
            payload: AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: ended_event_id,
                event_type: AwarenessEventType::AwarenessCycleEnded,
                occurred_at_ms: emission.final_event.base.timestamp_ms,
                awareness_cycle_id: emission.cycle_id,
                parent_cycle_id,
                collab_scope_id: emission.collab_scope_id.clone(),
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: ended_payload,
            },
        });

        Ok(OutboxEnvelope {
            tenant_id: emission.final_event.base.tenant_id,
            cycle_id: emission.cycle_id,
            messages,
        })
    }
}

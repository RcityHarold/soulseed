use serde_json::json;

use crate::errors::AceError;
use crate::types::{CycleEmission, OutboxEnvelope, OutboxMessage};
use soulseed_agi_core_models::awareness::{AwarenessEvent, AwarenessEventType};

pub struct Emitter;

impl Emitter {
    pub fn emit(&self, emission: CycleEmission) -> Result<OutboxEnvelope, AceError> {
        let mut messages: Vec<OutboxMessage> = emission
            .awareness_events
            .into_iter()
            .map(|payload| OutboxMessage {
                cycle_id: emission.cycle_id,
                event_id: payload.event_id,
                payload,
            })
            .collect();

        messages.push(OutboxMessage {
            cycle_id: emission.cycle_id,
            event_id: emission.final_event.event_id,
            payload: AwarenessEvent {
                anchor: emission.anchor.clone(),
                event_id: emission.final_event.event_id,
                event_type: AwarenessEventType::Finalized,
                occurred_at_ms: emission.final_event.timestamp_ms,
                awareness_cycle_id: emission.cycle_id,
                parent_cycle_id: None,
                collab_scope_id: None,
                barrier_id: None,
                env_mode: None,
                inference_cycle_sequence: 1,
                degradation_reason: None,
                payload: json!({
                    "final_event_id": emission.final_event.event_id.as_u64(),
                    "lane": format!("{:?}", emission.lane),
                }),
            },
        });

        Ok(OutboxEnvelope {
            tenant_id: emission.final_event.tenant_id,
            cycle_id: emission.cycle_id,
            messages,
        })
    }
}

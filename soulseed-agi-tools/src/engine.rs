use crate::config::ToolsConfig;
use crate::dto::{
    EngineInput, EngineOutput, PlannerContext, RouterCandidate, RouterOutput, RouterRequest,
    RouterState, ToolExecutionRecord, ToolHistoryStats, ToolLane,
};
use crate::errors::{EngineError, PlannerError, ToolError};
use crate::metrics::Observability;
use crate::traits::{Explainer, Orchestrator, Planner, Router, ToolEngineRunner};
use crate::tw_client::ThinWaistClient;
use serde_json::{json, Map, Value};
#[cfg(feature = "vectors-extra")]
use soulseed_agi_core_models::ExtraVectors;
use soulseed_agi_core_models::{
    CorrelationId, DialogueEvent, DialogueEventType, EnvelopeHead, RealTimePriority,
    SelfReflectionRecord, Snapshot, ToolInvocation, ToolResult, TraceId,
};
use soulseed_agi_core_models::legacy::dialogue_event::DialogueEvent as LegacyDialogueEvent;
use std::collections::HashMap;
use time::OffsetDateTime;
use xxhash_rust::xxh3::xxh3_64;

pub struct ToolEngine<'a> {
    pub router: &'a dyn Router,
    pub planner: &'a dyn Planner,
    pub orchestrator: &'a dyn Orchestrator,
    pub explainer: &'a dyn Explainer,
    pub config: ToolsConfig,
}

impl<'a> ToolEngine<'a> {
    fn augment_hints(base: &[String], lane: ToolLane) -> Vec<String> {
        let mut hints = base.to_vec();
        for extra in lane.extra_hints() {
            if !hints.iter().any(|h| h.eq_ignore_ascii_case(extra)) {
                hints.push((*extra).to_string());
            }
        }
        hints
    }

    fn build_router_state(
        &self,
        input: &EngineInput,
        client: &dyn ThinWaistClient,
        hints: &[String],
    ) -> Result<RouterState, EngineError> {
        let (defs, policy_digest) = client
            .tools_list(&input.anchor, &input.scene, hints)
            .map_err(|e| EngineError::Tool(ToolError::ThinWaist(e.to_string())))?;

        let stats_override = input
            .context_tags
            .get("stats")
            .and_then(serde_json::Value::as_object)
            .cloned();

        let candidates = defs
            .into_iter()
            .map(|def| {
                let overrides = stats_override
                    .as_ref()
                    .and_then(|map| map.get(&def.tool_id))
                    .and_then(serde_json::Value::as_object);

                let stats = ToolHistoryStats {
                    success_rate: overrides
                        .and_then(|o| o.get("success_rate"))
                        .and_then(serde_json::Value::as_f64)
                        .map(|v| v as f32)
                        .unwrap_or(0.7),
                    latency_p95: overrides
                        .and_then(|o| o.get("latency_p95"))
                        .and_then(serde_json::Value::as_f64)
                        .map(|v| v as f32)
                        .unwrap_or(0.8),
                    cost_per_success: overrides
                        .and_then(|o| o.get("cost_per_success"))
                        .and_then(serde_json::Value::as_f64)
                        .map(|v| v as f32)
                        .unwrap_or(1.0),
                    risk_level: overrides
                        .and_then(|o| o.get("risk_level"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(def.risk_level.as_str())
                        .to_string(),
                    auth_impact: overrides
                        .and_then(|o| o.get("auth_impact"))
                        .and_then(serde_json::Value::as_f64)
                        .map(|v| v as f32)
                        .unwrap_or(0.2),
                    cacheable: overrides
                        .and_then(|o| o.get("cacheable"))
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(!def.side_effect),
                    degradation_acceptance: overrides
                        .and_then(|o| o.get("degradation_acceptance"))
                        .and_then(serde_json::Value::as_f64)
                        .map(|v| v as f32)
                        .unwrap_or(0.5),
                };

                RouterCandidate {
                    tool_id: def.tool_id,
                    version: def.version,
                    capability: def.capability,
                    side_effect: def.side_effect,
                    supports_stream: def.supports_stream,
                    stats,
                }
            })
            .collect();

        Ok(RouterState {
            candidates,
            policy_digest,
        })
    }

    fn router_output(
        &self,
        request: &RouterRequest,
        state: &RouterState,
    ) -> Result<RouterOutput, EngineError> {
        let output = self.router.route(request, state, &self.config.router);
        Ok(output)
    }
}

impl<'a> ToolEngineRunner for ToolEngine<'a> {
    fn run(
        &self,
        input: EngineInput,
        client: &dyn ThinWaistClient,
        obs: &dyn Observability,
    ) -> Result<EngineOutput, EngineError> {
        if matches!(
            input.anchor.access_class,
            crate::dto::AccessClass::Restricted
        ) && input.anchor.provenance.is_none()
        {
            return Err(EngineError::Tool(ToolError::PrivacyRestricted));
        }
        self.config
            .validate()
            .map_err(|e| EngineError::ConfigInvalid(e.to_string()))?;

        let lane = input.anchor.lane(&input.scene);
        let capability_hints = Self::augment_hints(&input.capability_hints, lane);

        let router_state = self.build_router_state(&input, client, &capability_hints)?;
        let request = RouterRequest {
            anchor: input.anchor.clone(),
            scene: input.scene.clone(),
            capability_hints: capability_hints.clone(),
            context_tags: input.context_tags.clone(),
            subject: input.subject.clone(),
        };
        let router_output = self.router_output(&request, &router_state)?;
        if router_output.ranked.is_empty() {
            return Err(EngineError::Planner(PlannerError::NoCandidates));
        }
        obs.emit_metric(
            "tool_router_rank_latency_ms",
            1.0,
            &[
                ("scene", input.scene.clone()),
                ("lane", lane.metric_tag().to_string()),
            ],
        );

        let planner_ctx = PlannerContext {
            anchor: input.anchor.clone(),
            ranked: router_output.ranked,
            policy_digest: router_output.policy_digest,
            excluded: router_output.excluded,
            scene: input.scene.clone(),
            capability_hints,
            request_context: input.request_context.clone(),
            catalog: router_state.candidates,
            subject: input.subject.clone(),
        };
        let plan = self
            .planner
            .build_plan(planner_ctx, &self.config)
            .map_err(EngineError::Planner)?;

        obs.emit_metric(
            "tool_plan_build_latency_ms",
            1.0,
            &[
                ("scene", input.scene.clone()),
                ("lane", lane.metric_tag().to_string()),
            ],
        );

        let mut execution = self
            .orchestrator
            .execute_plan(&plan, obs)
            .map_err(EngineError::Orchestrator)?;
        self.explainer.decorate(&plan, &mut execution);

        let record = ToolExecutionRecord {
            summary: execution.summary.clone(),
            explain_run: execution.explain_run.clone(),
            fallback_triggered: execution.fallback_triggered,
            meta: execution.meta.clone(),
        };

        let dialogue_events = build_dialogue_events(&plan, &input.scene, &record);

        Ok(EngineOutput {
            plan,
            execution: record,
            dialogue_events,

        })
    }
}

fn build_dialogue_events(
    plan: &crate::dto::ToolPlan,
    scene: &str,
    execution: &ToolExecutionRecord,
) -> Vec<DialogueEvent> {
    use crate::dto::{ConversationScenario, Subject, ToolLane, ToolResultSummary};

    let Some(session_id) = plan.anchor.session_id else {
        return Vec::new();
    };

    let scenario = plan
        .anchor
        .scenario
        .clone()
        .unwrap_or(ConversationScenario::HumanToAi);
    let mut next_sequence = plan.anchor.sequence_number.unwrap_or(1);
    let now = OffsetDateTime::now_utc();
    let mut next_timestamp: i64 = (now.unix_timestamp_nanos() / 1_000_000) as i64;
    let subject = plan.subject.clone().unwrap_or(Subject::System);
    let lane_tag = plan.lane.metric_tag();

    let head = EnvelopeHead {
        envelope_id: plan.anchor.envelope_id.clone(),
        trace_id: TraceId(format!("tools:{}", plan.plan_id)),
        correlation_id: CorrelationId(plan.plan_id.clone()),
        config_snapshot_hash: plan.anchor.config_snapshot_hash.clone(),
        config_snapshot_version: plan.anchor.config_snapshot_version,
    };

    let snapshot = Snapshot {
        schema_v: plan.anchor.schema_v,
        created_at: now,
    };

    let mut summary_pool: Vec<ToolResultSummary> =
        execution.explain_run.collected_summaries.clone();
    let per_tool_degrade: HashMap<String, Option<String>> = execution
        .meta
        .per_tool_degradation
        .iter()
        .map(|(tool, reason)| (tool.clone(), reason.clone()))
        .collect();

    let mut events = Vec::new();

    for (idx, spec) in plan.items.iter().enumerate() {
        let stage = match execution
            .explain_run
            .stages
            .iter()
            .rev()
            .find(|stage| stage.tool_id == spec.tool_id)
            .cloned()
        {
            Some(stage) => stage,
            None => continue,
        };
        let stage_outcome = stage.outcome.clone();
        let stage_note = stage.notes.clone();

        let summary_index = summary_pool.iter().position(|s| s.tool_id == spec.tool_id);
        let summary = summary_index.map(|idx| summary_pool.remove(idx));
        let stage_latency_ms = stage
            .end_ms
            .saturating_sub(stage.start_ms)
            .min(u64::from(u32::MAX)) as u32;

        let call_event_legacy = LegacyDialogueEvent {
            tenant_id: plan.anchor.tenant_id,
            event_id: new_event_id(&plan.plan_id, "call", &spec.idem_key),
            session_id,
            subject: subject.clone(),
            participants: Vec::new(),
            head: head.clone(),
            snapshot: snapshot.clone(),
            timestamp_ms: next_timestamp,
            scenario: scenario.clone(),
            event_type: DialogueEventType::ToolCall,
            time_window: None,
            access_class: plan.anchor.access_class,
            provenance: plan.anchor.provenance.clone(),
            sequence_number: next_sequence,
            trigger_event_id: None,
            temporal_pattern_id: None,
            causal_links: Vec::new(),
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
            growth_stage: Some("tool:call".into()),
            processing_latency_ms: Some(stage_latency_ms),
            influence_score: None,
            community_impact: None,
            evidence_pointer: None,
            content_digest_sha256: None,
            blob_ref: None,
            supersedes: None,
            superseded_by: None,
            message_ref: None,
            tool_invocation: Some(ToolInvocation {
                tool_id: spec.tool_id.clone(),
                call_id: spec.idem_key.clone(),
                input: spec.input.clone(),
                strategy: Some(format!("{:?}", plan.strategy.mode)),
            }),
            tool_result: None,
            self_reflection: None,
            metadata: {
                let mut meta = Map::new();
                meta.insert("lane".into(), json!(lane_tag));
                meta.insert("params".into(), spec.params.clone());
                meta.insert("cacheable".into(), json!(spec.cacheable));
                meta.insert("stream".into(), json!(spec.stream));
                meta.insert("rank".into(), json!(idx + 1));
                if let Some(reason) = execution.explain_run.degradation_reason.as_ref() {
                    meta.insert("degradation_reason".into(), json!(reason));
                }
                if let Some(indices) = execution.explain_run.indices_used.as_ref() {
                    meta.insert("indices_used".into(), json!(indices));
                }
                if let Some(hash) = execution.explain_run.query_hash.as_ref() {
                    meta.insert("query_hash".into(), json!(hash));
                }
                Value::Object(meta)
            },
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        };
        let call_event = soulseed_agi_core_models::convert_legacy_dialogue_event(call_event_legacy);
        validate_event(&call_event);
        events.push(call_event);
        next_sequence += 1;
        next_timestamp += 1;

        let Some(summary) = summary else {
            continue;
        };
        let pointer = summary.evidence_pointer.clone();
        let pointer_uri = pointer.uri.clone();
        let result_digest = summary.result_digest.clone();
        let output_value = summary.summary.clone();
        let success = stage_outcome == "ok";

        let tool_result = ToolResult {
            tool_id: spec.tool_id.clone(),
            call_id: spec.idem_key.clone(),
            success,
            output: output_value,
            error: if success {
                None
            } else {
                Some(stage_outcome.clone())
            },
            degradation_reason: if success {
                per_tool_degrade
                    .get(&spec.tool_id)
                    .and_then(|reason| reason.clone())
                    .or_else(|| execution.explain_run.degradation_reason.clone())
            } else {
                stage_note.clone().or_else(|| Some(stage_outcome.clone()))
            },
        };
        let result_degrade = tool_result.degradation_reason.clone();

        let result_event_legacy = LegacyDialogueEvent {
            tenant_id: plan.anchor.tenant_id,
            event_id: new_event_id(&plan.plan_id, "result", &spec.idem_key),
            session_id,
            subject: subject.clone(),
            participants: Vec::new(),
            head: head.clone(),
            snapshot: snapshot.clone(),
            timestamp_ms: next_timestamp,
            scenario: scenario.clone(),
            event_type: DialogueEventType::ToolResult,
            time_window: None,
            access_class: plan.anchor.access_class,
            provenance: plan.anchor.provenance.clone(),
            sequence_number: next_sequence,
            trigger_event_id: None,
            temporal_pattern_id: None,
            causal_links: Vec::new(),
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
            growth_stage: Some("tool:result".into()),
            processing_latency_ms: Some(stage_latency_ms),
            influence_score: None,
            community_impact: None,
            evidence_pointer: Some(pointer.clone()),
            content_digest_sha256: Some(result_digest.clone()),
            blob_ref: None,
            supersedes: None,
            superseded_by: None,
            message_ref: None,
            tool_invocation: None,
            tool_result: Some(tool_result),
            self_reflection: None,
            metadata: {
                let mut meta = Map::new();
                meta.insert("lane".into(), json!(lane_tag));
                meta.insert("outcome".into(), json!(stage_outcome));
                meta.insert("notes".into(), json!(stage_note));
                meta.insert(
                    "fallback_triggered".into(),
                    json!(execution.fallback_triggered),
                );
                meta.insert("rank".into(), json!(idx + 1));
                if let Some(reason) = result_degrade.as_ref() {
                    meta.insert("degradation_reason".into(), json!(reason));
                } else if let Some(reason) = execution.explain_run.degradation_reason.as_ref() {
                    meta.insert("degradation_reason".into(), json!(reason));
                }
                if let Some(indices) = execution.explain_run.indices_used.as_ref() {
                    meta.insert("indices_used".into(), json!(indices));
                }
                if let Some(hash) = execution.explain_run.query_hash.as_ref() {
                    meta.insert("query_hash".into(), json!(hash));
                }
                meta.insert("evidence_uri".into(), json!(pointer_uri.clone()));
                Value::Object(meta)
            },
            #[cfg(feature = "vectors-extra")]
            vectors: ExtraVectors::default(),
        };
        let result_event =
            soulseed_agi_core_models::convert_legacy_dialogue_event(result_event_legacy);
        validate_event(&result_event);
        events.push(result_event);
        next_sequence += 1;
        next_timestamp += 1;
    }

    if matches!(plan.lane, ToolLane::SelfReflection) {
        if let Some(summary) = &execution.summary {
            let record = SelfReflectionRecord {
                topic: scene.to_string(),
                outcome: summary.summary.clone(),
                confidence: None,
            };
            let self_event_legacy = LegacyDialogueEvent {
                tenant_id: plan.anchor.tenant_id,
                event_id: new_event_id(&plan.plan_id, "self_reflection", &summary.tool_id),
                session_id,
                subject: subject.clone(),
                participants: Vec::new(),
                head,
                snapshot,
                timestamp_ms: next_timestamp,
                scenario: scenario.clone(),
                event_type: DialogueEventType::SelfReflection,
                time_window: None,
                access_class: plan.anchor.access_class,
                provenance: plan.anchor.provenance.clone(),
                sequence_number: next_sequence,
                trigger_event_id: None,
                temporal_pattern_id: None,
                causal_links: Vec::new(),
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
                real_time_priority: Some(RealTimePriority::Normal),
                notification_targets: None,
                live_stream_id: None,
                growth_stage: Some("self_reflection".into()),
                processing_latency_ms: None,
                influence_score: None,
                community_impact: None,
                evidence_pointer: None,
                content_digest_sha256: None,
                blob_ref: None,
                supersedes: None,
                superseded_by: None,
                message_ref: None,
                tool_invocation: None,
                tool_result: None,
                self_reflection: Some(record),
                metadata: {
                    let mut meta = Map::new();
                    meta.insert("lane".into(), json!(lane_tag));
                    meta.insert("source_tool".into(), json!(summary.tool_id.clone()));
                    if let Some(reason) = execution.explain_run.degradation_reason.as_ref() {
                        meta.insert("degradation_reason".into(), json!(reason));
                    }
                    if let Some(indices) = execution.explain_run.indices_used.as_ref() {
                        meta.insert("indices_used".into(), json!(indices));
                    }
                    if let Some(hash) = execution.explain_run.query_hash.as_ref() {
                        meta.insert("query_hash".into(), json!(hash));
                    }
                    Value::Object(meta)
                },
                #[cfg(feature = "vectors-extra")]
                vectors: ExtraVectors::default(),
            };
            let event = soulseed_agi_core_models::convert_legacy_dialogue_event(self_event_legacy);
            validate_event(&event);
            events.push(event);
        }
    } else if matches!(plan.lane, ToolLane::Collaboration) {
        if let Some(summary) = &execution.summary {
            let collab_pointer = summary.evidence_pointer.clone();
            let collab_event_legacy = LegacyDialogueEvent {
                tenant_id: plan.anchor.tenant_id,
                event_id: new_event_id(&plan.plan_id, "collaboration", &summary.tool_id),
                session_id,
                subject,
                participants: Vec::new(),
                head,
                snapshot,
                timestamp_ms: next_timestamp,
                scenario: scenario.clone(),
                event_type: DialogueEventType::ToolResult,
                time_window: None,
                access_class: plan.anchor.access_class,
                provenance: plan.anchor.provenance.clone(),
                sequence_number: next_sequence,
                trigger_event_id: None,
                temporal_pattern_id: None,
                causal_links: Vec::new(),
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
                real_time_priority: Some(RealTimePriority::Normal),
                notification_targets: None,
                live_stream_id: None,
                growth_stage: Some("tool:collaboration".into()),
                processing_latency_ms: None,
                influence_score: None,
                community_impact: None,
                evidence_pointer: Some(collab_pointer.clone()),
                content_digest_sha256: None,
                blob_ref: None,
                supersedes: None,
                superseded_by: None,
                message_ref: None,
                tool_invocation: None,
                tool_result: Some(ToolResult {
                    tool_id: summary.tool_id.clone(),
                    call_id: format!("summary:{}", summary.tool_id),
                    success: true,
                    output: summary.summary.clone(),
                    error: None,
                    degradation_reason: execution.explain_run.degradation_reason.clone(),
                }),
                self_reflection: None,
                metadata: {
                    let mut meta = Map::new();
                    meta.insert("lane".into(), json!(lane_tag));
                    meta.insert("summary".into(), json!(true));
                    if let Some(reason) = execution.explain_run.degradation_reason.as_ref() {
                        meta.insert("degradation_reason".into(), json!(reason));
                    }
                    if let Some(indices) = execution.explain_run.indices_used.as_ref() {
                        meta.insert("indices_used".into(), json!(indices));
                    }
                    if let Some(hash) = execution.explain_run.query_hash.as_ref() {
                        meta.insert("query_hash".into(), json!(hash));
                    }
                    meta.insert("evidence_uri".into(), json!(collab_pointer.uri.clone()));
                    Value::Object(meta)
                },
                #[cfg(feature = "vectors-extra")]
                vectors: ExtraVectors::default(),
            };
            let collab_event =
                soulseed_agi_core_models::convert_legacy_dialogue_event(collab_event_legacy);
            validate_event(&collab_event);
            events.push(collab_event);
        }
    }

    events
}

fn new_event_id(plan_id: &str, kind: &str, key: &str) -> crate::dto::EventId {
    crate::dto::EventId::from_raw_unchecked(xxh3_64(format!("{plan_id}:{kind}:{key}").as_bytes()))
}

#[cfg_attr(not(test), allow(dead_code))]
fn validate_event(event: &DialogueEvent) {
    if let Err(err) = soulseed_agi_core_models::validate_dialogue_event(event) {
        debug_assert!(false, "dialogue event validation failed: {:?}", err);
    }
}

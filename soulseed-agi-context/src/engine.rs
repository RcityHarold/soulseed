use std::collections::HashMap;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    assembly::{build_prefix, merge_delta},
    config::ContextConfig,
    errors::ContextError,
    traits::{
        Compressor, ContextStore, MCCPlanner, Observability, PointerValidator, QualityGate,
        ScoreAdapter,
    },
    types::{
        BundleItem, BundleScoreStats, BundleSegment, ContextBundle, ContextEvent, ContextItem,
        ContextItemDigests, DeltaPatch, EnvironmentSnapshotEvent, EvidencePointer, ExplainBundle,
        GraphExplain, Partition, PlanAction, PromptBundle, RedactionEntry, RedactionReport,
        RunInput, RunOutput, ScoredItem,
    },
    validate::{ensure_summary_anchor, validate_run},
};
use time::OffsetDateTime;

#[derive(Clone, Debug)]
struct ItemRecord {
    partition: Partition,
    ts_ms: i64,
    digests: ContextItemDigests,
    typ: Option<String>,
    supersedes: Option<String>,
    evidence_ptrs: Vec<EvidencePointer>,
    reasons: Vec<String>,
    score_stats: BundleScoreStats,
    score_scaled: i32,
    prompt_line: Option<String>,
}

impl ItemRecord {
    fn from_context_item(item: &ContextItem, score: &crate::types::ContextScore) -> Self {
        let mut digests = ensure_item_digests(item);
        if digests.semantic.is_none() {
            digests.semantic = item.digests.semantic.clone();
        }
        let ts_ms = unix_millis(item.observed_at);
        let mut score_stats = BundleScoreStats::default();
        score_stats.factors.insert("S".into(), score.final_score);
        score_stats.factors.insert("Rel".into(), score.relevance);
        score_stats.factors.insert("Cau".into(), score.importance);
        score_stats
            .factors
            .insert("Cmp".into(), score.compressibility);
        score_stats
            .factors
            .insert("Utility".into(), score.utility_density);

        let mut reasons = Vec::new();
        reasons.push("recall:initial".into());
        reasons.push(format!(
            "score:S_scaled={}",
            (score.final_score * 1000.0).round() as i32
        ));

        Self {
            partition: item.partition,
            ts_ms,
            digests,
            typ: item.typ.clone(),
            supersedes: item.links.supersedes.clone(),
            evidence_ptrs: item.links.evidence_ptrs.clone(),
            reasons,
            score_stats,
            score_scaled: (score.final_score * 1000.0).round() as i32,
            prompt_line: Some(value_to_prompt(&item.content)),
        }
    }

    fn from_summary(
        summary: &crate::types::SummaryUnit,
        partition: Partition,
        base_score: &crate::types::ContextScore,
    ) -> Self {
        let mut digests = ContextItemDigests::default();
        digests.content = Some(hash_json(&summary.summary));
        let ts_ms = unix_millis(OffsetDateTime::now_utc());
        let mut score_stats = BundleScoreStats::default();
        score_stats
            .factors
            .insert("S".into(), base_score.final_score);
        score_stats
            .factors
            .insert("Rel".into(), base_score.relevance);
        score_stats
            .factors
            .insert("Cmp".into(), base_score.compressibility);
        score_stats
            .factors
            .insert("Utility".into(), base_score.utility_density);

        let mut reasons = Vec::new();
        reasons.push("recall:summary".into());
        reasons.push("compress:applied".into());

        Self {
            partition,
            ts_ms,
            digests,
            typ: Some(format!("summary::{:?}", summary.level)),
            supersedes: summary.lineage.supersedes.clone(),
            evidence_ptrs: summary.evidence.iter().cloned().collect::<Vec<_>>(),
            reasons,
            score_stats,
            score_scaled: (base_score.final_score * 1000.0).round() as i32,
            prompt_line: Some(value_to_prompt(&summary.summary)),
        }
    }

    fn push_reason(&mut self, stage: &str, detail: impl Into<String>) {
        self.reasons.push(format!("{}:{}", stage, detail.into()));
    }

    fn why_included(&self) -> Option<String> {
        if self.reasons.is_empty() {
            None
        } else {
            Some(self.reasons.join("|"))
        }
    }

    fn to_bundle_item(
        &self,
        ci_id: String,
        tokens: u32,
        summary_level: Option<crate::types::Level>,
    ) -> BundleItem {
        BundleItem {
            ci_id,
            partition: self.partition,
            summary_level,
            tokens,
            score_scaled: self.score_scaled,
            ts_ms: self.ts_ms,
            digests: self.digests.clone(),
            typ: self.typ.clone(),
            why_included: self.why_included(),
            score_stats: self.score_stats.clone(),
            supersedes: self.supersedes.clone(),
            evidence_ptrs: self.evidence_ptrs.clone(),
        }
    }

    fn apply_prompt(&self, prompt: &mut PromptBundle) {
        let Some(line) = &self.prompt_line else {
            return;
        };
        match self.partition {
            Partition::P0Policy => prompt.system.push(line.clone()),
            Partition::P1TaskFacts => prompt.instructions.push(line.clone()),
            Partition::P2Evidence => prompt.facts.push(line.clone()),
            Partition::P3WorkingDelta => prompt.working.push(line.clone()),
            Partition::P4Dialogue => prompt.dialogue.push(line.clone()),
        }
    }
}

fn ensure_item_digests(item: &ContextItem) -> ContextItemDigests {
    if item.digests.content.is_some() {
        return item.digests.clone();
    }
    let mut digests = item.digests.clone();
    if digests.content.is_none() {
        digests.content = Some(hash_json(&item.content));
    }
    digests
}

fn hash_json(value: &Value) -> String {
    let serialized = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized);
    format!("sha256-{}", hex::encode(hasher.finalize()))
}

fn value_to_prompt(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "".into(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".into()),
    }
}

fn unix_millis(dt: OffsetDateTime) -> i64 {
    let nanos = dt.unix_timestamp_nanos();
    let max = (i64::MAX as i128) * 1_000_000;
    let min = (i64::MIN as i128) * 1_000_000;
    if nanos >= max {
        i64::MAX
    } else if nanos <= min {
        i64::MIN
    } else {
        (nanos / 1_000_000) as i64
    }
}

fn build_full_patch(bundle: &ContextBundle) -> DeltaPatch {
    let mut added = Vec::new();
    let mut score_stats = HashMap::new();
    let mut why_map = Map::new();
    let mut pointer_map = Map::new();

    for segment in &bundle.segments {
        for item in &segment.items {
            added.push(item.ci_id.clone());
            if let Some(score) = item.score_stats.factors.get("S") {
                score_stats.insert(item.ci_id.clone(), *score);
            }
            if let Some(reason) = &item.why_included {
                why_map.insert(item.ci_id.clone(), Value::String(reason.clone()));
            }
            if !item.evidence_ptrs.is_empty() {
                let ptrs: Vec<Value> = item
                    .evidence_ptrs
                    .iter()
                    .filter_map(|ptr| serde_json::to_value(ptr).ok())
                    .collect();
                pointer_map.insert(item.ci_id.clone(), Value::Array(ptrs));
            }
        }
    }

    let why_included = if why_map.is_empty() {
        None
    } else {
        Some(Value::Object(why_map))
    };

    let pointers = if pointer_map.is_empty() {
        None
    } else {
        Some(Value::Object(pointer_map))
    };

    let mut hasher = Sha256::new();
    hasher.update(format!("initial_patch|{}", bundle.version));
    for id in &added {
        hasher.update(b"add:");
        hasher.update(id.as_bytes());
    }
    let patch_digest = format!("dlt-{}", hex::encode(hasher.finalize()));

    DeltaPatch {
        patch_id: Uuid::now_v7().to_string(),
        added,
        updated: Vec::new(),
        removed: Vec::new(),
        score_stats,
        why_included,
        pointers,
        patch_digest,
    }
}

pub struct ContextEngine<'a> {
    pub scorer: &'a dyn ScoreAdapter,
    pub planner: &'a dyn MCCPlanner,
    pub compressor: &'a dyn Compressor,
    pub qgate: &'a dyn QualityGate,
    pub pointer: &'a dyn PointerValidator,
    pub store: &'a dyn ContextStore,
    pub obs: &'a dyn Observability,
}

impl<'a> ContextEngine<'a> {
    pub fn run(&self, input: RunInput) -> Result<RunOutput, ContextError> {
        let RunInput {
            anchor,
            env_context,
            config,
            items,
            graph_explain,
            previous_manifest,
        } = input;
        let graph_meta: GraphExplain = graph_explain.unwrap_or_default();
        let scenario_cfg = config.for_scenario(anchor.scenario.as_ref());
        validate_run(&anchor, &scenario_cfg, &items, &env_context)?;

        let env_ctx = env_context;
        let env_digest_tag = ("envctx_digest", env_ctx.context_digest.clone());
        let env_degradation = env_ctx.degradation_reason.clone();

        let scored = score_items(self.scorer, &scenario_cfg, &items);
        let mut item_records: HashMap<String, ItemRecord> = HashMap::new();
        for entry in &scored {
            item_records.insert(
                entry.item.id.clone(),
                ItemRecord::from_context_item(&entry.item, &entry.score),
            );
        }
        for record in item_records.values_mut() {
            record.push_reason("dedup", "unique");
            record.push_reason("privacy", "minimal_visible");
        }
        let score_index: HashMap<String, crate::types::ContextScore> = scored
            .iter()
            .map(|entry| (entry.item.id.clone(), entry.score.clone()))
            .collect();
        let mut plan = self.planner.plan(&anchor, &scenario_cfg, &scored);
        self.store.record_plan(&plan);

        let mut degrade_reasons: Vec<String> = Vec::new();
        let mut ask_consent_reasons: Vec<String> = Vec::new();

        let mut final_tokens_total: i64 = 0;
        let original_total: i64 = items.iter().map(|ci| ci.tokens as i64).sum();
        let mut rolled_back = Vec::new();
        let mut quality_stats = Vec::new();
        let mut segments: HashMap<Partition, Vec<BundleItem>> = HashMap::new();
        let mut prompt = PromptBundle::default();
        let mut pointer_index: HashMap<String, Vec<EvidencePointer>> = items
            .iter()
            .map(|ci| (ci.id.clone(), ci.links.evidence_ptrs.clone()))
            .collect();

        let index_map: HashMap<String, usize> = plan
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.ci_id.clone(), idx))
            .collect();

        for scored_item in &scored {
            let item = &scored_item.item;
            let partition = scored_item.score.partition_affinity;
            let Some(&plan_entry_idx) = index_map.get(&item.id) else {
                continue;
            };
            let initial_action = plan.items[plan_entry_idx].action;

            match initial_action {
                PlanAction::Keep => {
                    final_tokens_total += item.tokens as i64;
                    plan.items[plan_entry_idx].tokens_after = item.tokens;
                    refresh_plan_item_metrics(&mut plan.items[plan_entry_idx], &score_index);
                    plan.items[plan_entry_idx].trace.why_drop = None;
                    plan.items[plan_entry_idx].trace.why_compress = None;
                    plan.items[plan_entry_idx]
                        .trace
                        .why_keep
                        .get_or_insert_with(|| {
                            format!(
                                "retain_final:utility_density={:.4}",
                                scored_item.score.utility_density
                            )
                        });
                    if let Some(record) = item_records.get_mut(&item.id) {
                        record.partition = partition;
                        record.push_reason(
                            "trim",
                            format!("keep:density={:.4}", scored_item.score.utility_density),
                        );
                        let bundle_item = record.to_bundle_item(item.id.clone(), item.tokens, None);
                        record.apply_prompt(&mut prompt);
                        segments.entry(partition).or_default().push(bundle_item);
                    }
                }
                PlanAction::Evict => {
                    plan.items[plan_entry_idx].tokens_after = 0;
                    refresh_plan_item_metrics(&mut plan.items[plan_entry_idx], &score_index);
                    plan.items[plan_entry_idx].trace.why_keep = None;
                    plan.items[plan_entry_idx].trace.why_compress = None;
                    plan.items[plan_entry_idx]
                        .trace
                        .why_drop
                        .get_or_insert_with(|| {
                            format!(
                                "mcc_evict_low_density:density={:.4}",
                                scored_item.score.utility_density
                            )
                        });
                    if let Some(record) = item_records.get_mut(&item.id) {
                        record.partition = partition;
                        record.push_reason(
                            "trim",
                            format!("evict:density={:.4}", scored_item.score.utility_density),
                        );
                    }
                    degrade_reasons.push(format!("evicted_by_plan:{}", item.id));
                }
                PlanAction::L1 | PlanAction::L2 | PlanAction::L3 => {
                    let attempts = scenario_cfg.downgrade_actions(partition, initial_action);
                    let mut final_action = None;
                    let mut final_tokens = item.tokens;
                    let mut bundle_item = None;
                    let mut accepted_summary: Option<crate::types::SummaryUnit> = None;

                    for action in attempts {
                        match action {
                            PlanAction::Keep => {
                                final_action = Some(PlanAction::Keep);
                                final_tokens = item.tokens;
                                if let Some(record) = item_records.get_mut(&item.id) {
                                    record.partition = partition;
                                    record.push_reason("compress", "fallback_keep");
                                    record.apply_prompt(&mut prompt);
                                    bundle_item = Some(record.to_bundle_item(
                                        item.id.clone(),
                                        item.tokens,
                                        None,
                                    ));
                                } else {
                                    bundle_item = Some(BundleItem {
                                        ci_id: item.id.clone(),
                                        partition,
                                        summary_level: None,
                                        tokens: item.tokens,
                                        score_scaled: (scored_item.score.final_score * 1000.0)
                                            .round()
                                            as i32,
                                        ts_ms: unix_millis(item.observed_at),
                                        digests: ensure_item_digests(item),
                                        typ: item.typ.clone(),
                                        why_included: Some("compress:fallback".into()),
                                        score_stats: BundleScoreStats::default(),
                                        supersedes: item.links.supersedes.clone(),
                                        evidence_ptrs: item.links.evidence_ptrs.clone(),
                                    });
                                }
                                plan.items[plan_entry_idx]
                                    .trace
                                    .why_keep
                                    .get_or_insert_with(|| {
                                        format!(
                                            "mcc_fallback_keep:density={:.4}",
                                            scored_item.score.utility_density
                                        )
                                    });
                                degrade_reasons.push(format!("compression_fallback:{}", item.id));
                                break;
                            }
                            PlanAction::L1 | PlanAction::L2 | PlanAction::L3 => {
                                let outcome = self.compressor.apply(item, action);
                                if let Some(summary) = outcome.summary {
                                    pointer_index.insert(
                                        summary.su_id.clone(),
                                        summary.evidence.iter().cloned().collect::<Vec<_>>(),
                                    );
                                    ensure_summary_anchor(&anchor, &summary)?;
                                    if let Some(pointer) = &summary.evidence {
                                        self.pointer.validate(pointer)?;
                                    }
                                    match self.qgate.evaluate(&summary, &scenario_cfg) {
                                        Ok(()) => {
                                            final_action = Some(action);
                                            final_tokens = outcome.tokens_after;
                                            let summary_record = ItemRecord::from_summary(
                                                &summary,
                                                partition,
                                                &scored_item.score,
                                            );
                                            summary_record.apply_prompt(&mut prompt);
                                            bundle_item = Some(summary_record.to_bundle_item(
                                                summary.su_id.clone(),
                                                outcome.tokens_after,
                                                Some(summary.level),
                                            ));
                                            item_records
                                                .insert(summary.su_id.clone(), summary_record);
                                            quality_stats.push((
                                                summary.su_id.clone(),
                                                summary.quality.clone(),
                                            ));
                                            accepted_summary = Some(summary);
                                            plan.items[plan_entry_idx].trace.why_compress =
                                                Some(format!(
                                                    "quality_gate_pass:density={:.4}",
                                                    scored_item.score.utility_density
                                                ));
                                            break;
                                        }
                                        Err(failure) => {
                                            rolled_back.push((
                                                summary.su_id.clone(),
                                                failure.reason.clone(),
                                            ));
                                            degrade_reasons.push(format!(
                                                "quality_gate_fail:{}:{}",
                                                summary.su_id, failure.reason
                                            ));
                                            if failure.reason.contains("pointer") {
                                                ask_consent_reasons.push(format!(
                                                    "{}:{}",
                                                    summary.su_id, failure.reason
                                                ));
                                                plan.items[plan_entry_idx].trace.ask_consent = Some(
                                                    format!("quality_gate:{}", failure.reason),
                                                );
                                            }
                                            continue;
                                        }
                                    }
                                } else {
                                    rolled_back.push((
                                        item.id.clone(),
                                        format!("missing_summary:{:?}", action),
                                    ));
                                    degrade_reasons.push(format!(
                                        "compression_missing_summary:{}:{:?}",
                                        item.id, action
                                    ));
                                }
                            }
                            PlanAction::Evict => {
                                final_action = Some(PlanAction::Evict);
                                final_tokens = 0;
                                plan.items[plan_entry_idx]
                                    .trace
                                    .why_drop
                                    .get_or_insert_with(|| {
                                        format!(
                                            "evict_after_quality_fail:density={:.4}",
                                            scored_item.score.utility_density
                                        )
                                    });
                                degrade_reasons.push(format!("evicted_after_mcc:{}", item.id));
                                break;
                            }
                        }
                    }

                    if final_action.is_none() {
                        final_action = Some(PlanAction::Keep);
                        final_tokens = item.tokens;
                        if let Some(record) = item_records.get_mut(&item.id) {
                            record.partition = partition;
                            record.push_reason("compress", "fallback_keep_after_fail");
                            record.apply_prompt(&mut prompt);
                            bundle_item =
                                Some(record.to_bundle_item(item.id.clone(), item.tokens, None));
                        }
                        rolled_back.push((
                            item.id.clone(),
                            format!("fallback_keep:{:?}", initial_action),
                        ));
                        degrade_reasons.push(format!("compression_fallback:{}", item.id));
                        plan.items[plan_entry_idx].trace.why_drop = None;
                        plan.items[plan_entry_idx].trace.why_compress = None;
                        plan.items[plan_entry_idx]
                            .trace
                            .why_keep
                            .get_or_insert_with(|| {
                                format!(
                                    "fallback_keep_after_fail:density={:.4}",
                                    scored_item.score.utility_density
                                )
                            });
                    }

                    let final_action = final_action.unwrap();
                    plan.items[plan_entry_idx].action = final_action;
                    plan.items[plan_entry_idx].tokens_after = final_tokens;
                    refresh_plan_item_metrics(&mut plan.items[plan_entry_idx], &score_index);

                    if let Some(summary) = accepted_summary {
                        self.store.record_summary(&summary);
                    }

                    if let Some(item_bundle) = bundle_item {
                        final_tokens_total += final_tokens as i64;
                        segments.entry(partition).or_default().push(item_bundle);
                    } else {
                        final_tokens_total += final_tokens as i64;
                    }
                }
            }
        }

        let tokens_saved = original_total - final_tokens_total;

        let mut segments_vec: Vec<BundleSegment> = segments
            .into_iter()
            .map(|(partition, mut items)| {
                items.sort_by(|a, b| a.ci_id.cmp(&b.ci_id));
                BundleSegment { partition, items }
            })
            .collect();
        segments_vec
            .sort_by(|a, b| partition_order(b.partition).cmp(&partition_order(a.partition)));

        plan.budget.projected_tokens = final_tokens_total.max(0).min(u32::MAX as i64) as u32;

        let mut explain_reasons = graph_meta.reasons.clone();
        {
            fn push_reason(list: &mut Vec<String>, reason: String) {
                if !list.iter().any(|existing| existing == &reason) {
                    list.push(reason);
                }
            }
            push_reason(&mut explain_reasons, "plan_applied".into());
            if let Some(reason) = graph_meta.degradation_reason.as_ref() {
                push_reason(&mut explain_reasons, format!("graph_degraded:{reason}"));
            }
            if let Some(reason) = env_degradation.as_ref() {
                push_reason(
                    &mut explain_reasons,
                    format!("envctx_degraded:{}", reason.code),
                );
            }
            push_reason(
                &mut explain_reasons,
                format!("envctx_digest:{}", env_ctx.context_digest),
            );
            for reason in &degrade_reasons {
                push_reason(&mut explain_reasons, format!("ctx_degrade:{reason}"));
            }
            for reason in &ask_consent_reasons {
                push_reason(&mut explain_reasons, format!("ctx_ask_consent:{reason}"));
            }
        }

        let mut degradation_components: Vec<String> = Vec::new();
        if let Some(env) = env_degradation.as_ref() {
            degradation_components.push(format!("envctx:{}", env.code));
        }
        if let Some(graph) = graph_meta.degradation_reason.as_ref() {
            degradation_components.push(format!("graph:{}", graph));
        }
        if !degrade_reasons.is_empty() {
            degradation_components.push(format!("ctx:{}", degrade_reasons.join("|")));
        }
        if !ask_consent_reasons.is_empty() {
            degradation_components.push(format!("ask_consent:{}", ask_consent_reasons.join("|")));
        }
        let degradation_summary = if degradation_components.is_empty() {
            None
        } else {
            Some(degradation_components.join(";"))
        };

        let bundle = ContextBundle {
            anchor: anchor.clone(),
            schema_v: anchor.schema_v,
            version: plan.lineage.version,
            segments: segments_vec,
            explain: ExplainBundle {
                reasons: explain_reasons,
                degradation_reason: degradation_summary.clone(),
                indices_used: graph_meta.indices_used.clone(),
                query_hash: graph_meta.query_hash.clone(),
            },
            budget: plan.budget.clone(),
            prompt,
        };

        let now = OffsetDateTime::now_utc();
        let (manifest, delta_patch) = if let Some(prev_manifest) = previous_manifest {
            let merged = merge_delta(&prev_manifest, &bundle)
                .map_err(|err| ContextError::ManifestInvariant(format!("delta_merge:{err}")))?;
            self.store.record_event(&ContextEvent::DeltaMerged {
                anchor: anchor.clone(),
                schema_v: anchor.schema_v,
                patch_id: merged.patch.patch_id.clone(),
                manifest_digest: merged.manifest.manifest_digest.clone(),
                at: now,
            });
            (merged.manifest, merged.patch)
        } else {
            let manifest = build_prefix(&bundle)
                .map_err(|err| ContextError::ManifestInvariant(format!("manifest_build:{err}")))?;
            self.store.record_event(&ContextEvent::ContextBuilt {
                anchor: anchor.clone(),
                schema_v: anchor.schema_v,
                plan_id: plan.plan_id.clone(),
                manifest_digest: manifest.manifest_digest.clone(),
                at: now,
            });
            let patch = build_full_patch(&bundle);
            (manifest, patch)
        };

        let prompt_digest = hash_json(&serde_json::to_value(&bundle.prompt).unwrap_or(Value::Null));
        self.store.record_event(&ContextEvent::PromptIssued {
            anchor: anchor.clone(),
            schema_v: anchor.schema_v,
            manifest_digest: manifest.manifest_digest.clone(),
            prompt_digest,
            at: now,
        });

        let rollback_count = rolled_back.len() as f64;

        let report = crate::types::CompressionReport {
            anchor: anchor.clone(),
            schema_v: anchor.schema_v,
            plan_id: plan.plan_id.clone(),
            tokens_saved,
            rolled_back,
            quality_stats,
            degradation_reason: degradation_summary.clone(),
        };
        self.store.record_report(&report);

        let tenant_tag = ("tenant_id", anchor.tenant_id.into_inner().to_string());
        let scenario_label = anchor
            .scenario
            .as_ref()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "Unspecified".into());
        let scenario_tag = ("scenario", scenario_label.clone());

        self.obs.emit_metric(
            "ctx_token_saving_total",
            tokens_saved as f64,
            &[
                tenant_tag.clone(),
                scenario_tag.clone(),
                env_digest_tag.clone(),
            ],
        );

        self.obs.emit_metric(
            "context_bundle_tokens",
            plan.budget.projected_tokens as f64,
            &[
                tenant_tag.clone(),
                scenario_tag.clone(),
                env_digest_tag.clone(),
            ],
        );

        self.obs.emit_metric(
            "compression_rollback",
            rollback_count,
            &[
                tenant_tag.clone(),
                scenario_tag.clone(),
                env_digest_tag.clone(),
            ],
        );

        if !degrade_reasons.is_empty() {
            self.obs.emit_metric(
                "ctx_degrade_total",
                degrade_reasons.len() as f64,
                &[
                    tenant_tag.clone(),
                    scenario_tag.clone(),
                    env_digest_tag.clone(),
                ],
            );
        }

        if !ask_consent_reasons.is_empty() {
            self.obs.emit_metric(
                "ctx_ask_consent_total",
                ask_consent_reasons.len() as f64,
                &[
                    tenant_tag.clone(),
                    scenario_tag.clone(),
                    env_digest_tag.clone(),
                ],
            );
        }

        if let Some(reason) = env_degradation.as_ref() {
            self.obs.emit_metric(
                "ctx_env_degrade_total",
                1.0,
                &[
                    tenant_tag.clone(),
                    scenario_tag.clone(),
                    env_digest_tag.clone(),
                    ("code", reason.code.clone()),
                ],
            );
        }

        if let Some(reason) = graph_meta.degradation_reason.as_ref() {
            self.obs.emit_metric(
                "ctx_graph_degrade_total",
                1.0,
                &[
                    tenant_tag.clone(),
                    scenario_tag,
                    env_digest_tag,
                    ("reason", reason.clone()),
                ],
            );
        }

        let mut evidence_pointers: Vec<EvidencePointer> = pointer_index
            .values()
            .flat_map(|list| list.clone())
            .collect();
        evidence_pointers.sort_by(|a, b| a.uri.cmp(&b.uri));
        evidence_pointers.dedup();

        let base_snapshot = soulseed_agi_envctx::build_snapshot_event(&env_ctx);
        for pointer in &base_snapshot.evidence {
            if !evidence_pointers.iter().any(|p| p.uri == pointer.uri) {
                evidence_pointers.push(pointer.clone());
            }
        }
        evidence_pointers.sort_by(|a, b| a.uri.cmp(&b.uri));
        evidence_pointers.dedup();

        let env_snapshot = EnvironmentSnapshotEvent {
            anchor: anchor.clone(),
            schema_v: base_snapshot.schema_v,
            context_digest: base_snapshot.context_digest.clone(),
            snapshot_digest: base_snapshot.snapshot_digest.clone(),
            lite_mode: base_snapshot.lite_mode,
            environment: base_snapshot.environment.clone(),
            region: base_snapshot.region.clone(),
            scene: base_snapshot.scene.clone(),
            risk_flag: base_snapshot.risk_flag.clone(),
            manifest_digest: manifest.manifest_digest.clone(),
            source_versions: base_snapshot.source_versions.clone(),
            policy_digest: base_snapshot.policy_digest.clone(),
            evidence_pointers,
            degradation_reason: base_snapshot
                .degradation_reason
                .as_ref()
                .map(|reason| reason.code.clone())
                .or(degradation_summary.clone()),
            generated_at: base_snapshot.generated_at,
        };

        let action_map: HashMap<String, PlanAction> = plan
            .items
            .iter()
            .map(|item| (item.ci_id.clone(), item.action.clone()))
            .collect();

        let redaction_entries: Vec<RedactionEntry> = report
            .rolled_back
            .iter()
            .map(|(ci_id, reason)| RedactionEntry {
                ci_id: ci_id.clone(),
                reason: reason.clone(),
                evidence_pointer: pointer_index
                    .get(ci_id)
                    .and_then(|list| list.first().cloned()),
                final_action: action_map.get(ci_id).cloned(),
            })
            .collect();

        let redaction = RedactionReport {
            anchor: anchor.clone(),
            schema_v: anchor.schema_v,
            manifest_digest: manifest.manifest_digest.clone(),
            entries: redaction_entries,
        };
        self.store.record_redaction(&redaction);
        self.store.record_event(&ContextEvent::RedactionReported {
            anchor: anchor.clone(),
            schema_v: anchor.schema_v,
            manifest_digest: manifest.manifest_digest.clone(),
            at: OffsetDateTime::now_utc(),
        });

        Ok(RunOutput {
            plan,
            report,
            bundle,
            delta_patch,
            manifest,
            env_snapshot,
            redaction,
        })
    }
}

fn score_items(
    scorer: &dyn ScoreAdapter,
    cfg: &ContextConfig,
    items: &[ContextItem],
) -> Vec<ScoredItem> {
    items
        .iter()
        .map(|item| ScoredItem {
            score: scorer.score(item, cfg),
            item: item.clone(),
        })
        .collect()
}

fn refresh_plan_item_metrics(
    item: &mut crate::types::PlanItem,
    score_index: &HashMap<String, crate::types::ContextScore>,
) {
    if let Some(score) = score_index.get(&item.ci_id) {
        let saved = item.base_tokens as i32 - item.tokens_after as i32;
        let base = item.base_tokens.max(1);
        let ratio = (saved.max(0) as f32) / (base as f32);
        item.estimated_tokens_saved = saved;
        item.utility_delta = (score.final_score.max(0.0)) * ratio;
    }
}

fn partition_order(partition: Partition) -> u8 {
    match partition {
        Partition::P0Policy => 5,
        Partition::P1TaskFacts => 4,
        Partition::P2Evidence => 3,
        Partition::P3WorkingDelta => 2,
        Partition::P4Dialogue => 1,
    }
}

impl<'a> crate::traits::ContextEngineRunner for ContextEngine<'a> {
    fn run(&self, input: RunInput) -> Result<RunOutput, ContextError> {
        ContextEngine::run(self, input)
    }
}

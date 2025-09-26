use std::collections::HashMap;

use crate::{
    assembly::{build_prefix, ContextManifest},
    config::ContextConfig,
    errors::ContextError,
    traits::{
        Compressor, ContextStore, MCCPlanner, Observability, PointerValidator, QualityGate,
        ScoreAdapter,
    },
    types::{
        BundleItem, BundleSegment, ContextBundle, ContextItem, EnvironmentSnapshotEvent,
        EvidencePointer, ExplainBundle, GraphExplain, Partition, PlanAction, RedactionEntry,
        RedactionReport, RunInput, RunOutput, ScoredItem,
    },
    validate::{ensure_summary_anchor, validate_run},
};
use time::OffsetDateTime;

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
        } = input;
        let graph_meta: GraphExplain = graph_explain.unwrap_or_default();
        let scenario_cfg = config.for_scenario(anchor.scenario.as_ref());
        validate_run(&anchor, &scenario_cfg, &items, &env_context)?;

        let env_ctx = env_context;
        let env_digest_tag = ("envctx_digest", env_ctx.context_digest.clone());
        let env_degradation = env_ctx.degradation_reason.clone();

        let scored = score_items(self.scorer, &scenario_cfg, &items);
        let mut plan = self.planner.plan(&anchor, &scenario_cfg, &scored);
        self.store.record_plan(&plan);

        let mut final_tokens_total: i64 = 0;
        let original_total: i64 = items.iter().map(|ci| ci.tokens as i64).sum();
        let mut rolled_back = Vec::new();
        let mut quality_stats = Vec::new();
        let mut segments: HashMap<Partition, Vec<BundleItem>> = HashMap::new();
        let mut pointer_index: HashMap<String, Option<EvidencePointer>> = items
            .iter()
            .map(|ci| (ci.id.clone(), ci.evidence.clone()))
            .collect();

        let index_map: HashMap<String, usize> = plan
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.ci_id.clone(), idx))
            .collect();

        for scored_item in &scored {
            let item = &scored_item.item;
            let partition = item.partition_hint.unwrap_or(Partition::P4Dialogue);
            let Some(&plan_entry_idx) = index_map.get(&item.id) else {
                continue;
            };
            let initial_action = plan.items[plan_entry_idx].action;

            match initial_action {
                PlanAction::Keep => {
                    final_tokens_total += item.tokens as i64;
                    plan.items[plan_entry_idx].tokens_after = item.tokens;
                    segments.entry(partition).or_default().push(BundleItem {
                        ci_id: item.id.clone(),
                        summary_level: None,
                        tokens: item.tokens,
                    });
                }
                PlanAction::Evict => {
                    plan.items[plan_entry_idx].tokens_after = 0;
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
                                bundle_item = Some(BundleItem {
                                    ci_id: item.id.clone(),
                                    summary_level: None,
                                    tokens: item.tokens,
                                });
                                break;
                            }
                            PlanAction::L1 | PlanAction::L2 | PlanAction::L3 => {
                                let outcome = self.compressor.apply(item, action);
                                if let Some(summary) = outcome.summary {
                                    pointer_index
                                        .insert(summary.su_id.clone(), summary.evidence.clone());
                                    ensure_summary_anchor(&anchor, &summary)?;
                                    if let Some(pointer) = &summary.evidence {
                                        self.pointer.validate(pointer)?;
                                    }
                                    match self.qgate.evaluate(&summary) {
                                        Ok(()) => {
                                            final_action = Some(action);
                                            final_tokens = outcome.tokens_after;
                                            bundle_item = Some(BundleItem {
                                                ci_id: summary.su_id.clone(),
                                                summary_level: Some(summary.level),
                                                tokens: outcome.tokens_after,
                                            });
                                            quality_stats.push((
                                                summary.su_id.clone(),
                                                summary.quality.clone(),
                                            ));
                                            accepted_summary = Some(summary);
                                            break;
                                        }
                                        Err(failure) => {
                                            rolled_back.push((
                                                summary.su_id.clone(),
                                                failure.reason.clone(),
                                            ));
                                            continue;
                                        }
                                    }
                                } else {
                                    rolled_back.push((
                                        item.id.clone(),
                                        format!("missing_summary:{:?}", action),
                                    ));
                                }
                            }
                            PlanAction::Evict => {
                                final_action = Some(PlanAction::Evict);
                                final_tokens = 0;
                                break;
                            }
                        }
                    }

                    if final_action.is_none() {
                        final_action = Some(PlanAction::Keep);
                        final_tokens = item.tokens;
                        bundle_item = Some(BundleItem {
                            ci_id: item.id.clone(),
                            summary_level: None,
                            tokens: item.tokens,
                        });
                        rolled_back.push((
                            item.id.clone(),
                            format!("fallback_keep:{:?}", initial_action),
                        ));
                    }

                    let final_action = final_action.unwrap();
                    plan.items[plan_entry_idx].action = final_action;
                    plan.items[plan_entry_idx].tokens_after = final_tokens;

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
        segments_vec.sort_by_key(|seg| partition_order(seg.partition));

        plan.budget.projected_tokens = final_tokens_total.max(0) as u32;

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
        }

        let degradation_summary = match (
            env_degradation.as_ref(),
            graph_meta.degradation_reason.as_ref(),
        ) {
            (None, None) => None,
            (None, Some(graph)) => Some(graph.clone()),
            (Some(env), None) => Some(format!("envctx:{}", env.code)),
            (Some(env), Some(graph)) => Some(format!("envctx:{}|graph:{}", env.code, graph)),
        };

        let bundle = ContextBundle {
            anchor: anchor.clone(),
            segments: segments_vec,
            explain: ExplainBundle {
                reasons: explain_reasons,
                degradation_reason: degradation_summary.clone(),
                indices_used: graph_meta.indices_used.clone(),
                query_hash: graph_meta.query_hash.clone(),
            },
            budget: plan.budget.clone(),
        };

        let manifest: ContextManifest = build_prefix(&bundle)
            .map_err(|err| ContextError::ManifestInvariant(format!("manifest_build:{err}")))?;

        let rollback_count = rolled_back.len() as f64;

        let report = crate::types::CompressionReport {
            anchor: anchor.clone(),
            plan_id: plan.plan_id.clone(),
            tokens_saved,
            rolled_back,
            quality_stats,
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

        let mut evidence_pointers: Vec<EvidencePointer> =
            items.iter().filter_map(|ci| ci.evidence.clone()).collect();
        evidence_pointers.sort_by(|a, b| a.uri.cmp(&b.uri));
        evidence_pointers.dedup();

        let policy_digest = env_ctx.external_systems.policy_digest.trim();
        let policy_digest = if policy_digest.is_empty() {
            None
        } else {
            Some(policy_digest.to_string())
        };

        let env_snapshot = EnvironmentSnapshotEvent {
            anchor: anchor.clone(),
            context_digest: env_ctx.context_digest.clone(),
            snapshot_digest: extract_snapshot_digest(&env_ctx.context_digest),
            environment: env_ctx.external_systems.environment.clone(),
            region: env_ctx.external_systems.region.clone(),
            scene: Some(env_ctx.internal_scene.conversation.scene.clone()),
            risk_flag: format!("{:?}", env_ctx.internal_scene.risk_flag),
            manifest_digest: manifest.manifest_digest.clone(),
            source_versions: env_ctx.source_versions.clone(),
            policy_digest,
            evidence_pointers,
            degradation_reason: env_degradation.as_ref().map(|reason| reason.code.clone()),
            generated_at: OffsetDateTime::now_utc(),
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
                evidence_pointer: pointer_index.get(ci_id).cloned().unwrap_or(None),
                final_action: action_map.get(ci_id).cloned(),
            })
            .collect();

        let redaction = RedactionReport {
            anchor: anchor.clone(),
            manifest_digest: manifest.manifest_digest.clone(),
            entries: redaction_entries,
        };

        Ok(RunOutput {
            plan,
            report,
            bundle,
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

fn partition_order(partition: Partition) -> u8 {
    match partition {
        Partition::P0Policy => 0,
        Partition::P1TaskFacts => 1,
        Partition::P2Evidence => 2,
        Partition::P3WorkingDelta => 3,
        Partition::P4Dialogue => 4,
    }
}

fn extract_snapshot_digest(digest: &str) -> String {
    digest
        .split('|')
        .find(|part| part.trim_start().starts_with("blake3:"))
        .map(|part| part.trim().to_string())
        .unwrap_or_else(|| digest.to_string())
}

impl<'a> crate::traits::ContextEngineRunner for ContextEngine<'a> {
    fn run(&self, input: RunInput) -> Result<RunOutput, ContextError> {
        ContextEngine::run(self, input)
    }
}

use std::collections::HashMap;

use xxhash_rust::xxh3::xxh3_64_with_seed;

use crate::{
    config::{ContextConfig, PartitionQuota},
    types::{Anchor, ContextScore, Partition, PartitionUsage, PlanAction, PlanItem, ScoredItem},
};

pub struct DeterministicPlanner;

#[derive(Clone, Debug)]
struct PlanState {
    item: PlanItem,
    score: ContextScore,
    allowed: Vec<PlanAction>,
}

impl DeterministicPlanner {
    fn eviction_order() -> [Partition; 5] {
        [
            Partition::P4Dialogue,
            Partition::P3WorkingDelta,
            Partition::P2Evidence,
            Partition::P1TaskFacts,
            Partition::P0Policy,
        ]
    }

    fn action_order(action: &PlanAction) -> u8 {
        match action {
            PlanAction::Keep => 0,
            PlanAction::L1 => 1,
            PlanAction::L2 => 2,
            PlanAction::L3 => 3,
            PlanAction::Evict => 4,
        }
    }

    fn next_action(current: &PlanAction, allowed: &[PlanAction]) -> Option<PlanAction> {
        let idx = allowed.iter().position(|a| a == current)?;
        allowed.get(idx + 1).cloned()
    }

    fn tokens_after(tokens: u32, action: PlanAction) -> u32 {
        match action {
            PlanAction::Keep => tokens,
            PlanAction::Evict => 0,
            PlanAction::L1 => ((tokens as f32) * 0.65).ceil() as u32,
            PlanAction::L2 => ((tokens as f32) * 0.40).ceil() as u32,
            PlanAction::L3 => ((tokens as f32) * 0.25).ceil() as u32,
        }
    }

    fn partition_priority(p: Partition) -> u8 {
        match p {
            Partition::P0Policy => 0,
            Partition::P1TaskFacts => 1,
            Partition::P2Evidence => 2,
            Partition::P3WorkingDelta => 3,
            Partition::P4Dialogue => 4,
        }
    }

    fn compute_partition_usage(states: &[PlanState]) -> HashMap<Partition, (u32, u32)> {
        let mut partitions: HashMap<Partition, (u32, u32)> = HashMap::new();
        for state in states {
            let entry = partitions.entry(state.item.partition).or_insert((0, 0));
            entry.0 += state.item.base_tokens;
            entry.1 += state.item.tokens_after;
        }
        partitions
    }

    fn quota_for(cfg: &ContextConfig, partition: &Partition) -> PartitionQuota {
        cfg.partition_quota
            .get(partition)
            .copied()
            .unwrap_or_else(|| PartitionQuota {
                max_tokens: u32::MAX,
                min_tokens: 0,
            })
    }
}

impl crate::traits::MCCPlanner for DeterministicPlanner {
    fn plan(
        &self,
        anchor: &Anchor,
        cfg: &ContextConfig,
        items: &[ScoredItem],
    ) -> crate::types::CompressionPlan {
        let mut states: Vec<PlanState> = items
            .iter()
            .map(|sc| {
                let partition = sc.item.partition_hint.unwrap_or(Partition::P4Dialogue);
                PlanState {
                    item: PlanItem {
                        ci_id: sc.item.id.clone(),
                        partition,
                        base_tokens: sc.item.tokens,
                        action: PlanAction::Keep,
                        tokens_after: sc.item.tokens,
                    },
                    score: sc.score.clone(),
                    allowed: cfg.allowed_actions(partition),
                }
            })
            .collect();

        let mut usage = Self::compute_partition_usage(&states);
        let mut total_tokens: u32 = states.iter().map(|s| s.item.tokens_after).sum();

        loop {
            let mut progress = false;
            let mut partitions_need_reduce: Vec<Partition> = Vec::new();

            for partition in Self::eviction_order().iter() {
                if usage
                    .get(partition)
                    .map(|(_, after)| *after > Self::quota_for(cfg, partition).max_tokens)
                    .unwrap_or(false)
                {
                    partitions_need_reduce.push(*partition);
                }
            }

            if total_tokens > cfg.target_tokens {
                for partition in Self::eviction_order() {
                    if !partitions_need_reduce.contains(&partition) {
                        partitions_need_reduce.push(partition);
                    }
                }
            }

            if partitions_need_reduce.is_empty() {
                break;
            }

            for partition in partitions_need_reduce {
                let quota = Self::quota_for(cfg, &partition);
                let current_after = usage.get(&partition).map(|(_, after)| *after).unwrap_or(0);
                let mut candidate: Option<usize> = None;
                let mut candidate_score = f32::MAX;
                for (idx, state) in states.iter().enumerate() {
                    if state.item.partition != partition {
                        continue;
                    }
                    let Some(next) = Self::next_action(&state.item.action, &state.allowed) else {
                        continue;
                    };
                    let next_tokens_after = Self::tokens_after(state.item.base_tokens, next);
                    let reduction = state.item.tokens_after.saturating_sub(next_tokens_after);
                    if reduction == 0 {
                        continue;
                    }
                    if current_after.saturating_sub(reduction) < quota.min_tokens {
                        continue;
                    }
                    if state.score.final_score < candidate_score {
                        candidate = Some(idx);
                        candidate_score = state.score.final_score;
                    }
                }

                if let Some(idx) = candidate {
                    let next =
                        Self::next_action(&states[idx].item.action, &states[idx].allowed).unwrap();
                    states[idx].item.action = next;
                    states[idx].item.tokens_after =
                        Self::tokens_after(states[idx].item.base_tokens, states[idx].item.action);
                    usage = Self::compute_partition_usage(&states);
                    total_tokens = states.iter().map(|s| s.item.tokens_after).sum();
                    progress = true;
                    break;
                }
            }

            if !progress {
                break;
            }
        }

        let mut partitions_vec = usage
            .into_iter()
            .map(|(partition, (before, after))| PartitionUsage {
                partition,
                tokens_before: before,
                tokens_after: after,
            })
            .collect::<Vec<_>>();
        partitions_vec.sort_by_key(|u| Self::partition_priority(u.partition));

        let mut items_vec = states.into_iter().map(|s| s.item).collect::<Vec<_>>();
        items_vec.sort_by(|a, b| a.ci_id.cmp(&b.ci_id));

        let mut hasher_input = Vec::new();
        hasher_input.extend_from_slice(&anchor.tenant_id.into_inner().to_le_bytes());
        hasher_input.extend_from_slice(cfg.snapshot_hash.as_bytes());
        hasher_input.extend_from_slice(&cfg.snapshot_version.to_le_bytes());
        hasher_input.extend_from_slice(&cfg.target_tokens.to_le_bytes());
        for item in &items_vec {
            hasher_input.extend_from_slice(item.ci_id.as_bytes());
            hasher_input.push(Self::partition_priority(item.partition));
            hasher_input.push(Self::action_order(&item.action));
        }
        let plan_hash = xxh3_64_with_seed(&hasher_input, cfg.plan_seed);

        crate::types::CompressionPlan {
            plan_id: format!("plan-{:016x}", plan_hash),
            anchor: anchor.clone(),
            items: items_vec,
            partitions: partitions_vec,
            budget: crate::types::BudgetSummary {
                target_tokens: cfg.target_tokens,
                projected_tokens: total_tokens,
            },
        }
    }
}

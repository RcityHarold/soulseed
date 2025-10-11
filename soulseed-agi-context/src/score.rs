use time::Duration;

use crate::{
    config::ContextConfig,
    traits::ScoreAdapter,
    types::{ContextItem, ContextScore, FeatureVec, Partition},
};

pub struct ScoreAdapterHalfLife;

impl ScoreAdapterHalfLife {
    fn decay(age_hours: f32, half_life_hours: f32) -> f32 {
        if half_life_hours <= f32::EPSILON {
            return 1.0;
        }
        (-age_hours / half_life_hours.max(0.1))
            .exp()
            .clamp(0.0, 1.0)
    }

    fn age_hours(cfg: &ContextConfig, item: &ContextItem) -> f32 {
        let delta: Duration = cfg.scoring_reference - item.observed_at;
        delta.abs().as_seconds_f32() / 3600.0
    }

    fn infer_partition(item: &ContextItem, features: &FeatureVec) -> Partition {
        if let Some(hint) = item.partition_hint {
            return hint;
        }
        let declared = item.partition;
        if declared != Partition::P4Dialogue {
            return declared;
        }
        if !item.links.evidence_ptrs.is_empty() {
            return Partition::P2Evidence;
        }
        if let Some(item_type) = item.typ.as_deref() {
            let lowered = item_type.to_ascii_lowercase();
            if lowered.contains("policy") || lowered.contains("guard") {
                return Partition::P0Policy;
            }
            if lowered.contains("task") || lowered.contains("fact") {
                return Partition::P1TaskFacts;
            }
            if lowered.contains("delta") || lowered.contains("draft") {
                return Partition::P3WorkingDelta;
            }
        }

        if features.auth >= 0.75 && features.risk >= 0.5 {
            return Partition::P0Policy;
        }
        if features.rel >= 0.65 && features.cau >= 0.55 {
            return Partition::P1TaskFacts;
        }
        if features.stab <= 0.4 && features.dup <= 0.3 {
            return Partition::P3WorkingDelta;
        }
        declared
    }

    fn utility_density(final_score: f32, tokens: u32) -> f32 {
        if tokens == 0 {
            return 0.0;
        }
        (final_score.max(0.0)) / (tokens as f32)
    }
}

impl ScoreAdapter for ScoreAdapterHalfLife {
    fn score(&self, item: &ContextItem, cfg: &ContextConfig) -> ContextScore {
        let features = item.features.clone().clamp();
        let inferred_partition = Self::infer_partition(item, &features);

        let age_hours = Self::age_hours(cfg, item);
        let half_life = cfg.half_life_hours(inferred_partition);
        let decay = Self::decay(age_hours, half_life);

        let rel = (features.rel * (0.5 + 0.5 * decay)).clamp(0.0, 1.0);
        let cau = features.cau.clamp(0.0, 1.0);
        let rec = (features.rec * decay).clamp(0.0, 1.0);
        let auth = (features.auth * (0.7 + 0.3 * decay)).clamp(0.0, 1.0);
        let stab = features.stab.clamp(0.0, 1.0);
        let dup = features.dup.clamp(0.0, 1.0);
        let len = features.len.clamp(0.0, 1.0);
        let risk = features.risk.clamp(0.0, 1.0);

        let w = &cfg.score_weights;
        let weighted = rel * w.rel
            + cau * w.cau
            + rec * w.rec
            + auth * w.auth
            + stab * w.stab
            + dup * w.dup
            + len * w.len
            + risk * w.risk;

        let final_score = weighted.clamp(-1.0, 1.0);
        let importance = ((rel * 0.5 + auth * 0.3 + cau * 0.2).max(0.0)).clamp(0.0, 1.0);
        let compressibility = ((1.0 - len.max(dup)) * (1.0 - risk * 0.5)).clamp(0.0, 1.0);
        let utility_density = Self::utility_density(final_score, item.tokens.max(1));

        ContextScore {
            relevance: rel,
            importance,
            compressibility,
            final_score,
            partition_affinity: inferred_partition,
            utility_density,
        }
    }
}

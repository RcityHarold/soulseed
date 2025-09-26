use crate::{
    config::ContextConfig,
    traits::ScoreAdapter,
    types::{ContextItem, ContextScore, FeatureVec, Partition},
};

pub struct ScoreAdapterSimple;

impl ScoreAdapterSimple {
    fn weighted_sum(features: &FeatureVec, cfg: &ContextConfig) -> f32 {
        let w = &cfg.score_weights;
        features.rel * w.rel
            + features.cau * w.cau
            + features.rec * w.rec
            + features.auth * w.auth
            + features.stab * w.stab
            + features.dup * w.dup
            + features.len * w.len
            + (1.0 - features.risk) * w.risk
    }
}

impl ScoreAdapter for ScoreAdapterSimple {
    fn score(&self, item: &ContextItem, cfg: &ContextConfig) -> ContextScore {
        let features = item.features.clone().clamp();
        let final_score = Self::weighted_sum(&features, cfg).clamp(-1.0, 1.0);
        ContextScore {
            relevance: features.rel,
            importance: (features.rel + features.cau + features.auth) / 3.0,
            compressibility: 1.0 - features.len.max(features.dup),
            final_score,
            partition_affinity: item.partition_hint.unwrap_or(Partition::P4Dialogue),
        }
    }
}

use rand::rngs::SmallRng;
use rand::Rng;
use rand::SeedableRng;
use serde_json::json;
use xxhash_rust::xxh3::xxh3_64;

use crate::config::RouterConfig;
use crate::dto::{CandidateScore, RouterOutput, RouterRequest, RouterState, ToolHistoryStats};
use crate::traits::Router;

pub struct RouterDeterministic;

impl RouterDeterministic {
    fn risk_to_level(risk: &str) -> u8 {
        match risk.to_ascii_lowercase().as_str() {
            "very_low" => 0,
            "low" => 1,
            "medium" => 2,
            "high" => 3,
            "critical" => 4,
            _ => 2,
        }
    }

    fn context_fit(hints: &[String], capability: &[String]) -> f32 {
        if hints.is_empty() {
            return 1.0;
        }
        let matches = hints
            .iter()
            .filter(|h| capability.iter().any(|c| c.eq_ignore_ascii_case(h)))
            .count();
        (matches as f32 / hints.len() as f32).clamp(0.0, 1.0)
    }

    fn latency_score(stats: &ToolHistoryStats) -> f32 {
        1.0 / (1.0 + stats.latency_p95.max(0.0))
    }

    fn cost_score(stats: &ToolHistoryStats) -> f32 {
        1.0 / (1.0 + stats.cost_per_success.max(0.0))
    }

    fn risk_score(stats: &ToolHistoryStats) -> f32 {
        (1.0 - Self::risk_to_level(&stats.risk_level) as f32 / 4.0).clamp(0.0, 1.0)
    }

    fn cache_score(stats: &ToolHistoryStats) -> f32 {
        if stats.cacheable {
            1.0
        } else {
            0.0
        }
    }

    fn degrade_score(stats: &ToolHistoryStats) -> f32 {
        stats.degradation_acceptance.clamp(0.0, 1.0)
    }

    fn score_candidate(
        cfg: &RouterConfig,
        hints: &[String],
        candidate: &crate::dto::RouterCandidate,
    ) -> f32 {
        let stats = &candidate.stats;
        let weights = &cfg.weights;
        let context_fit = Self::context_fit(hints, &candidate.capability);
        let latency = Self::latency_score(stats);
        let cost = Self::cost_score(stats);
        let risk = Self::risk_score(stats);
        let cache = Self::cache_score(stats);
        let degrade = Self::degrade_score(stats);
        let success = stats.success_rate.clamp(0.0, 1.0);
        let auth = (1.0 - stats.auth_impact).clamp(0.0, 1.0);

        (context_fit * weights.context_fit)
            + (success * weights.success_rate)
            + (latency * weights.latency_p95)
            + (cost * weights.cost_per_success)
            + (risk * weights.risk_level)
            + (auth * weights.auth_impact)
            + (cache * weights.cacheability)
            + (degrade * weights.degradation_acceptance)
    }

    fn exploration_penalty(seed: u64, tool_id: &str, explore_rate: f32) -> f32 {
        if explore_rate <= 0.0 {
            return 0.0;
        }
        let hash = xxh3_64(
            [seed.to_le_bytes().as_slice(), tool_id.as_bytes()]
                .concat()
                .as_slice(),
        );
        let mut rng = SmallRng::seed_from_u64(hash);
        let sample: f32 = rng.gen();
        if sample < explore_rate {
            rng.gen::<f32>() * 0.1
        } else {
            0.0
        }
    }
}

impl Router for RouterDeterministic {
    fn route(&self, req: &RouterRequest, state: &RouterState, cfg: &RouterConfig) -> RouterOutput {
        let lane = req.anchor.lane(&req.scene);
        let mut hints = req.capability_hints.clone();
        for extra in lane.extra_hints() {
            if !hints.iter().any(|h| h.eq_ignore_ascii_case(extra)) {
                hints.push((*extra).to_string());
            }
        }

        let base_threshold = Self::risk_to_level(&cfg.risk_threshold);
        let threshold = lane.adjust_risk_gate(base_threshold);
        let seed = xxh3_64(
            [
                req.anchor.tenant_id.into_inner().to_le_bytes().as_ref(),
                &req.anchor.envelope_id.as_u128().to_le_bytes(),
                req.scene.as_bytes(),
                lane.metric_tag().as_bytes(),
            ]
            .concat()
            .as_slice(),
        ) ^ cfg.plan_seed;

        let mut ranked = Vec::new();
        let mut excluded = Vec::new();

        for candidate in &state.candidates {
            let candidate_risk = Self::risk_to_level(&candidate.stats.risk_level);
            if candidate_risk > threshold {
                excluded.push((candidate.tool_id.clone(), "risk_threshold".into()));
                continue;
            }
            if candidate.side_effect && cfg.max_candidates == 0 {
                excluded.push((candidate.tool_id.clone(), "side_effect_disallowed".into()));
                continue;
            }
            let mut score = Self::score_candidate(cfg, &hints, candidate);
            let penalty = Self::exploration_penalty(seed, &candidate.tool_id, cfg.explore_rate);
            score -= penalty;
            ranked.push((candidate.clone(), score));
        }

        ranked.sort_by(|(a, score_a), (b, score_b)| {
            score_b
                .partial_cmp(score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.tool_id.cmp(&b.tool_id))
        });

        let ranked = ranked
            .into_iter()
            .take(cfg.max_candidates)
            .enumerate()
            .map(|(idx, (candidate, score))| CandidateScore {
                tool_id: candidate.tool_id,
                score,
                rank: idx as u16 + 1,
                factors: json!({
                    "context_fit": Self::context_fit(&hints, &candidate.capability),
                    "success_rate": candidate.stats.success_rate,
                    "latency": Self::latency_score(&candidate.stats),
                    "cost": Self::cost_score(&candidate.stats),
                    "risk": Self::risk_score(&candidate.stats),
                    "auth": 1.0 - candidate.stats.auth_impact,
                    "cacheability": Self::cache_score(&candidate.stats),
                    "degradation_acceptance": candidate.stats.degradation_acceptance,
                    "lane": lane.metric_tag(),
                }),
            })
            .collect();

        RouterOutput {
            ranked,
            excluded,
            policy_digest: state.policy_digest.clone(),
        }
    }
}

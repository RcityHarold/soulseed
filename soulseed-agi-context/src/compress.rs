use serde_json::json;

use crate::{
    traits::{CompressionOutcome, Compressor},
    types::{ContextItem, Level, Lineage, PlanAction, QualityStats, SummaryUnit},
};

pub struct CompressorMock;

fn ratio(action: &PlanAction) -> f32 {
    match action {
        PlanAction::Keep => 1.0,
        PlanAction::Evict => 0.0,
        PlanAction::L1 => 0.65,
        PlanAction::L2 => 0.40,
        PlanAction::L3 => 0.25,
    }
}

impl Compressor for CompressorMock {
    fn apply(&self, item: &ContextItem, action: PlanAction) -> CompressionOutcome {
        match action {
            PlanAction::Keep => CompressionOutcome {
                tokens_after: item.tokens,
                summary: None,
            },
            PlanAction::Evict => CompressionOutcome {
                tokens_after: 0,
                summary: None,
            },
            PlanAction::L1 | PlanAction::L2 | PlanAction::L3 => {
                let ratio = ratio(&action);
                let tokens_after = ((item.tokens as f32) * ratio).ceil() as u32;
                let tokens_after = tokens_after.max(1);
                let tokens_saved = item.tokens.saturating_sub(tokens_after);
                let level = match action {
                    PlanAction::L1 => Level::L1,
                    PlanAction::L2 => Level::L2,
                    PlanAction::L3 => Level::L3,
                    _ => Level::L0,
                };
                let summary = SummaryUnit {
                    anchor: item.anchor.clone(),
                    su_id: format!("su:{}:{}", item.id, level as u8),
                    from_ids: vec![item.id.clone()],
                    level,
                    summary: json!({"summary_of": item.id, "level": format!("{:?}", level)}),
                    tokens_saved,
                    quality: QualityStats {
                        qag: 1.0,
                        coverage: 1.0,
                        nli_contrad: 0.0,
                        pointer_ok: item.evidence.is_some(),
                    },
                    lineage: Lineage {
                        version: 1,
                        supersedes: None,
                    },
                    evidence: item.evidence.clone(),
                };
                CompressionOutcome {
                    tokens_after,
                    summary: Some(summary),
                }
            }
        }
    }
}

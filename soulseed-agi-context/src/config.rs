use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::PlanAction;
use crate::{Level, Partition};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub rel: f32,
    pub cau: f32,
    pub rec: f32,
    pub auth: f32,
    pub stab: f32,
    pub dup: f32,
    pub len: f32,
    pub risk: f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            rel: 0.35,
            cau: 0.15,
            rec: 0.10,
            auth: 0.10,
            stab: 0.10,
            dup: -0.10,
            len: -0.05,
            risk: -0.10,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualityThresholds {
    pub qag: f32,
    pub coverage: f32,
    pub nli_contrad: f32,
    pub pointer_ok: f32,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        Self {
            qag: 0.90,
            coverage: 0.90,
            nli_contrad: 0.01,
            pointer_ok: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PartitionQuota {
    pub max_tokens: u32,
    pub min_tokens: u32,
}

impl PartitionQuota {
    pub const fn new(max_tokens: u32, min_tokens: u32) -> Self {
        Self {
            max_tokens,
            min_tokens,
        }
    }
}

impl Default for PartitionQuota {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            min_tokens: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextConfig {
    pub snapshot_hash: String,
    pub snapshot_version: u32,
    pub target_tokens: u32,
    pub partition_quota: HashMap<Partition, PartitionQuota>,
    pub score_weights: ScoreWeights,
    pub compression_allowlist: HashMap<Partition, Vec<Level>>,
    pub quality_thresholds: QualityThresholds,
    pub plan_seed: u64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        use Partition::*;
        let mut partition_quota = HashMap::new();
        partition_quota.insert(P0Policy, PartitionQuota::new(96, 32));
        partition_quota.insert(P1TaskFacts, PartitionQuota::new(256, 64));
        partition_quota.insert(P2Evidence, PartitionQuota::new(512, 64));
        partition_quota.insert(P3WorkingDelta, PartitionQuota::new(256, 0));
        partition_quota.insert(P4Dialogue, PartitionQuota::new(256, 0));

        let mut allow = HashMap::new();
        allow.insert(P0Policy, vec![Level::L0, Level::L1]);
        allow.insert(P1TaskFacts, vec![Level::L0, Level::L1]);
        allow.insert(P2Evidence, vec![Level::L0, Level::L1, Level::L2, Level::L3]);
        allow.insert(P3WorkingDelta, vec![Level::L0, Level::L1, Level::L2]);
        allow.insert(P4Dialogue, vec![Level::L0, Level::L1, Level::L2]);

        Self {
            snapshot_hash: "cfg-default".into(),
            snapshot_version: 1,
            target_tokens: 1024,
            partition_quota,
            score_weights: ScoreWeights::default(),
            compression_allowlist: allow,
            quality_thresholds: QualityThresholds::default(),
            plan_seed: 0,
        }
    }
}

impl ContextConfig {
    fn action_order(action: &PlanAction) -> u8 {
        match action {
            PlanAction::Keep => 0,
            PlanAction::L1 => 1,
            PlanAction::L2 => 2,
            PlanAction::L3 => 3,
            PlanAction::Evict => 4,
        }
    }

    pub fn allowed_actions(&self, partition: Partition) -> Vec<PlanAction> {
        let mut actions = Vec::new();
        actions.push(PlanAction::Keep);
        if let Some(levels) = self.compression_allowlist.get(&partition) {
            for level in levels {
                match level {
                    Level::L0 => {}
                    Level::L1 => actions.push(PlanAction::L1),
                    Level::L2 => actions.push(PlanAction::L2),
                    Level::L3 => actions.push(PlanAction::L3),
                }
            }
        }
        if matches!(
            partition,
            Partition::P2Evidence | Partition::P3WorkingDelta | Partition::P4Dialogue
        ) {
            actions.push(PlanAction::Evict);
        }
        actions.sort_by_key(Self::action_order);
        actions.dedup();
        actions
    }

    pub fn downgrade_actions(&self, partition: Partition, from: PlanAction) -> Vec<PlanAction> {
        let mut allowed = self.allowed_actions(partition);
        allowed.retain(|a| Self::action_order(a) <= Self::action_order(&from));
        allowed.reverse();
        allowed
    }

    pub fn for_scenario(
        &self,
        scenario: Option<&soulseed_agi_core_models::ConversationScenario>,
    ) -> Self {
        let mut cfg = self.clone();
        if let Some(s) = scenario {
            use soulseed_agi_core_models::ConversationScenario::*;
            use Partition::*;
            match s {
                HumanToAi | HumanToMultiAi => {
                    cfg.partition_quota
                        .insert(P4Dialogue, PartitionQuota::new(512, 96));
                    cfg.partition_quota
                        .insert(P1TaskFacts, PartitionQuota::new(224, 64));
                }
                AiSelfTalk => {
                    cfg.partition_quota
                        .insert(P3WorkingDelta, PartitionQuota::new(384, 64));
                    cfg.partition_quota
                        .insert(P4Dialogue, PartitionQuota::new(192, 0));
                }
                AiToSystem => {
                    cfg.partition_quota
                        .insert(P1TaskFacts, PartitionQuota::new(320, 96));
                    cfg.partition_quota
                        .insert(P0Policy, PartitionQuota::new(128, 32));
                }
                _ => {}
            }
        }
        cfg
    }
}

use crate::{
    config::{ContextConfig, PartitionQuota},
    errors::ContextError,
    types::{AccessClass, Anchor, ContextItem, SummaryUnit},
};
use soulseed_agi_envctx::EnvironmentContext;

pub fn validate_run(
    anchor: &Anchor,
    cfg: &ContextConfig,
    items: &[ContextItem],
    env_ctx: &EnvironmentContext,
) -> Result<(), ContextError> {
    validate_anchor(anchor, cfg)?;
    validate_config(cfg)?;
    ensure_env_anchor(anchor, env_ctx)?;
    if env_ctx.context_digest.is_empty() {
        return Err(ContextError::PlannerFailure("envctx_digest_missing".into()));
    }
    for item in items {
        ensure_anchor_match(anchor, &item.anchor)?;
    }
    Ok(())
}

pub fn validate_anchor(anchor: &Anchor, cfg: &ContextConfig) -> Result<(), ContextError> {
    if anchor.tenant_id.into_inner() == 0 {
        return Err(ContextError::PlannerFailure("tenant_id_missing".into()));
    }
    if anchor.schema_v == 0 {
        return Err(ContextError::PlannerFailure(
            "schema_version_invalid".into(),
        ));
    }
    if anchor.config_snapshot_hash != cfg.snapshot_hash {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_hash",
        });
    }
    if anchor.config_snapshot_version != cfg.snapshot_version {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_version",
        });
    }
    if anchor.access_class == AccessClass::Restricted && anchor.provenance.is_none() {
        return Err(ContextError::PrivacyRestricted);
    }
    Ok(())
}

pub fn ensure_anchor_match(expected: &Anchor, actual: &Anchor) -> Result<(), ContextError> {
    if expected.tenant_id != actual.tenant_id {
        return Err(ContextError::AnchorMismatch { field: "tenant_id" });
    }
    if expected.envelope_id != actual.envelope_id {
        return Err(ContextError::AnchorMismatch {
            field: "envelope_id",
        });
    }
    if expected.config_snapshot_hash != actual.config_snapshot_hash {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_hash",
        });
    }
    if expected.config_snapshot_version != actual.config_snapshot_version {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_version",
        });
    }
    if expected.session_id != actual.session_id {
        return Err(ContextError::AnchorMismatch {
            field: "session_id",
        });
    }
    if expected.sequence_number != actual.sequence_number {
        return Err(ContextError::AnchorMismatch {
            field: "sequence_number",
        });
    }
    if expected.access_class != actual.access_class {
        return Err(ContextError::AnchorMismatch {
            field: "access_class",
        });
    }
    if expected.provenance != actual.provenance {
        return Err(ContextError::AnchorMismatch {
            field: "provenance",
        });
    }
    if expected.scenario != actual.scenario {
        return Err(ContextError::AnchorMismatch { field: "scenario" });
    }
    if expected.schema_v != actual.schema_v {
        return Err(ContextError::AnchorMismatch { field: "schema_v" });
    }
    Ok(())
}

pub fn ensure_summary_anchor(expected: &Anchor, summary: &SummaryUnit) -> Result<(), ContextError> {
    ensure_anchor_match(expected, &summary.anchor)
}

fn ensure_env_anchor(anchor: &Anchor, env_ctx: &EnvironmentContext) -> Result<(), ContextError> {
    let target = &env_ctx.anchor;
    if anchor.tenant_id != target.tenant_id {
        return Err(ContextError::AnchorMismatch { field: "tenant_id" });
    }
    if anchor.envelope_id != target.envelope_id {
        return Err(ContextError::AnchorMismatch {
            field: "envelope_id",
        });
    }
    if anchor.config_snapshot_hash != target.config_snapshot_hash {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_hash",
        });
    }
    if anchor.config_snapshot_version != target.config_snapshot_version {
        return Err(ContextError::AnchorMismatch {
            field: "config_snapshot_version",
        });
    }
    if anchor.session_id != target.session_id {
        return Err(ContextError::AnchorMismatch {
            field: "session_id",
        });
    }
    if anchor.sequence_number != target.sequence_number {
        return Err(ContextError::AnchorMismatch {
            field: "sequence_number",
        });
    }
    if anchor.access_class != target.access_class {
        return Err(ContextError::AnchorMismatch {
            field: "access_class",
        });
    }
    if anchor.provenance != target.provenance {
        return Err(ContextError::AnchorMismatch {
            field: "provenance",
        });
    }
    if anchor.schema_v != target.schema_v {
        return Err(ContextError::AnchorMismatch { field: "schema_v" });
    }
    Ok(())
}

fn validate_config(cfg: &ContextConfig) -> Result<(), ContextError> {
    if cfg.target_tokens == 0 {
        return Err(ContextError::PlannerFailure("target_tokens_zero".into()));
    }
    for (partition, quota) in &cfg.partition_quota {
        validate_quota(partition, quota)?;
    }
    Ok(())
}

fn validate_quota(
    partition: &crate::types::Partition,
    quota: &PartitionQuota,
) -> Result<(), ContextError> {
    if quota.min_tokens > quota.max_tokens {
        return Err(ContextError::PlannerFailure(format!(
            "partition_min_gt_max:{partition:?}:{min}>{max}",
            partition = partition,
            min = quota.min_tokens,
            max = quota.max_tokens
        )));
    }
    Ok(())
}

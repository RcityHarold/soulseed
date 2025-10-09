use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use soulseed_agi_core_models::DeltaPatch;
use uuid::Uuid;

use crate::{
    errors::ContextError,
    types::{
        Anchor, BudgetSummary, BundleItem, BundleSegment, ContextBundle, Partition, Partition::*,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextManifest {
    pub anchor: Anchor,
    pub version: u32,
    pub working_generation: u32,
    pub segments: Vec<BundleSegment>,
    pub budget: BudgetSummary,
    pub manifest_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeReport {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    pub ignored: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionReport {
    pub version_from: u32,
    pub version_to: u32,
    pub removed_zero_tokens: Vec<String>,
    pub deduplicated: Vec<String>,
}

pub struct DeltaMergeOutput {
    pub manifest: ContextManifest,
    pub patch: DeltaPatch,
    pub report: MergeReport,
}

pub fn build_prefix(bundle: &ContextBundle) -> Result<ContextManifest, ContextError> {
    ensure_partitions(&bundle.segments)?;

    let mut segments = bundle.segments.clone();
    sort_segments(&mut segments);

    let manifest_digest = compute_manifest_digest(&bundle.anchor, 1, 0, &segments, &bundle.budget)?;

    Ok(ContextManifest {
        anchor: bundle.anchor.clone(),
        version: 1,
        working_generation: 0,
        segments,
        budget: bundle.budget.clone(),
        manifest_digest,
    })
}

pub fn merge_delta(
    manifest: &ContextManifest,
    delta_bundle: &ContextBundle,
) -> Result<DeltaMergeOutput, ContextError> {
    ensure_anchor_match(&manifest.anchor, &delta_bundle.anchor)?;

    let mut segments = delta_bundle.segments.clone();
    sort_segments(&mut segments);

    let current_items = segments_to_item_map(&manifest.segments);
    let new_items = segments_to_item_map(&segments);

    let mut added = Vec::new();
    let mut updated = Vec::new();
    let mut removed = Vec::new();

    for (id, item) in &new_items {
        match current_items.get(id) {
            None => added.push(id.clone()),
            Some(prev) => {
                if manifest_item_signature(prev) != manifest_item_signature(item) {
                    updated.push(id.clone());
                }
            }
        }
    }
    for id in current_items.keys() {
        if !new_items.contains_key(id) {
            removed.push(id.clone());
        }
    }

    let new_generation = manifest.working_generation.saturating_add(1);
    let manifest_digest = compute_manifest_digest(
        &manifest.anchor,
        manifest.version,
        new_generation,
        &segments,
        &delta_bundle.budget,
    )?;

    let new_manifest = ContextManifest {
        anchor: manifest.anchor.clone(),
        version: manifest.version,
        working_generation: new_generation,
        segments,
        budget: delta_bundle.budget.clone(),
        manifest_digest,
    };

    let mut score_stats = HashMap::new();
    for id in added.iter().chain(updated.iter()) {
        if let Some(item) = new_items.get(id) {
            if let Some(score) = item.score_stats.factors.get("S") {
                score_stats.insert(id.clone(), *score);
            }
        }
    }

    let mut why_map = Map::new();
    for (id, item) in &new_items {
        if let Some(reason) = &item.why_included {
            why_map.insert(id.clone(), Value::String(reason.clone()));
        }
    }
    let why_included = if why_map.is_empty() {
        None
    } else {
        Some(Value::Object(why_map))
    };

    let mut pointer_map = Map::new();
    for (id, item) in &new_items {
        if !item.evidence_ptrs.is_empty() {
            let ptrs: Vec<Value> = item
                .evidence_ptrs
                .iter()
                .filter_map(|ptr| serde_json::to_value(ptr).ok())
                .collect();
            pointer_map.insert(id.clone(), Value::Array(ptrs));
        }
    }
    let pointers = if pointer_map.is_empty() {
        None
    } else {
        Some(Value::Object(pointer_map))
    };

    let mut patch_hasher = Sha256::new();
    patch_hasher.update(format!(
        "{}|{}|{}",
        manifest.anchor.tenant_id.into_inner(),
        manifest.anchor.envelope_id,
        new_generation
    ));
    for id in &added {
        patch_hasher.update(b"add:");
        patch_hasher.update(id.as_bytes());
    }
    for id in &updated {
        patch_hasher.update(b"upd:");
        patch_hasher.update(id.as_bytes());
    }
    for id in &removed {
        patch_hasher.update(b"rem:");
        patch_hasher.update(id.as_bytes());
    }
    let patch_digest = format!("dlt-{}", hex::encode(patch_hasher.finalize()));

    let patch = DeltaPatch {
        patch_id: Uuid::now_v7().to_string(),
        added: added.clone(),
        updated: updated.clone(),
        removed: removed.clone(),
        score_stats,
        why_included,
        pointers,
        patch_digest,
    };

    let report = MergeReport {
        added,
        updated,
        removed,
        ignored: Vec::new(),
    };

    Ok(DeltaMergeOutput {
        manifest: new_manifest,
        patch,
        report,
    })
}

pub fn compact(manifest: &ContextManifest) -> (ContextManifest, CompactionReport) {
    let mut removed_zero = Vec::new();
    let mut deduped = Vec::new();

    let mut segments_map = segments_to_map(&manifest.segments);

    for (_partition, items) in segments_map.iter_mut() {
        let mut seen = HashSet::new();
        items.retain(|item| {
            if item.tokens == 0 {
                removed_zero.push(item.ci_id.clone());
                return false;
            }
            if !seen.insert(item.ci_id.clone()) {
                deduped.push(item.ci_id.clone());
                return false;
            }
            true
        });
    }

    let mut segments = map_to_segments(segments_map);
    sort_segments(&mut segments);

    let new_version = manifest.version.saturating_add(1);
    let manifest_digest = compute_manifest_digest(
        &manifest.anchor,
        new_version,
        0,
        &segments,
        &manifest.budget,
    )
    .expect("manifest digest");

    let new_manifest = ContextManifest {
        anchor: manifest.anchor.clone(),
        version: new_version,
        working_generation: 0,
        segments,
        budget: manifest.budget.clone(),
        manifest_digest,
    };

    let report = CompactionReport {
        version_from: manifest.version,
        version_to: new_version,
        removed_zero_tokens: removed_zero,
        deduplicated: deduped,
    };

    (new_manifest, report)
}

fn ensure_partitions(segments: &[BundleSegment]) -> Result<(), ContextError> {
    let mut seen = HashSet::new();
    for seg in segments {
        if !matches!(
            seg.partition,
            P0Policy | P1TaskFacts | P2Evidence | P3WorkingDelta | P4Dialogue
        ) {
            return Err(ContextError::ManifestInvariant(format!(
                "unknown_partition:{:?}",
                seg.partition
            )));
        }
        if !seen.insert(seg.partition) {
            return Err(ContextError::ManifestInvariant(format!(
                "duplicate_partition:{:?}",
                seg.partition
            )));
        }
    }
    Ok(())
}

fn ensure_anchor_match(expected: &Anchor, actual: &Anchor) -> Result<(), ContextError> {
    if expected.tenant_id != actual.tenant_id {
        return Err(ContextError::DeltaMismatch("tenant_id_mismatch".into()));
    }
    if expected.envelope_id != actual.envelope_id {
        return Err(ContextError::DeltaMismatch("envelope_id_mismatch".into()));
    }
    if expected.config_snapshot_hash != actual.config_snapshot_hash {
        return Err(ContextError::DeltaMismatch(
            "config_snapshot_hash_mismatch".into(),
        ));
    }
    if expected.config_snapshot_version != actual.config_snapshot_version {
        return Err(ContextError::DeltaMismatch(
            "config_snapshot_version_mismatch".into(),
        ));
    }
    if expected.session_id != actual.session_id {
        return Err(ContextError::DeltaMismatch("session_id_mismatch".into()));
    }
    if expected.sequence_number != actual.sequence_number {
        return Err(ContextError::DeltaMismatch(
            "sequence_number_mismatch".into(),
        ));
    }
    if expected.access_class != actual.access_class {
        return Err(ContextError::DeltaMismatch("access_class_mismatch".into()));
    }
    if expected.provenance != actual.provenance {
        return Err(ContextError::DeltaMismatch("provenance_mismatch".into()));
    }
    if expected.scenario != actual.scenario {
        return Err(ContextError::DeltaMismatch("scenario_mismatch".into()));
    }
    if expected.schema_v != actual.schema_v {
        return Err(ContextError::DeltaMismatch("schema_v_mismatch".into()));
    }
    Ok(())
}

fn segments_to_map(segments: &[BundleSegment]) -> HashMap<Partition, Vec<BundleItem>> {
    segments
        .iter()
        .map(|seg| (seg.partition, seg.items.clone()))
        .collect()
}

fn segments_to_item_map(segments: &[BundleSegment]) -> HashMap<String, BundleItem> {
    let mut map = HashMap::new();
    for seg in segments {
        for item in &seg.items {
            let mut cloned = item.clone();
            cloned.partition = seg.partition;
            map.insert(cloned.ci_id.clone(), cloned);
        }
    }
    map
}

fn map_to_segments(map: HashMap<Partition, Vec<BundleItem>>) -> Vec<BundleSegment> {
    map.into_iter()
        .map(|(partition, mut items)| {
            for item in &mut items {
                item.partition = partition;
            }
            items.sort_by(manifest_item_cmp);
            BundleSegment { partition, items }
        })
        .collect()
}

fn sort_segments(segments: &mut Vec<BundleSegment>) {
    for seg in segments.iter_mut() {
        for item in &mut seg.items {
            item.partition = seg.partition;
        }
        seg.items.sort_by(manifest_item_cmp);
    }
    segments.sort_by(|a, b| {
        manifest_partition_priority(b.partition).cmp(&manifest_partition_priority(a.partition))
    });
}

fn manifest_item_cmp(a: &BundleItem, b: &BundleItem) -> Ordering {
    manifest_partition_priority(b.partition)
        .cmp(&manifest_partition_priority(a.partition))
        .then_with(|| b.score_scaled.cmp(&a.score_scaled))
        .then_with(|| b.ts_ms.cmp(&a.ts_ms))
        .then_with(|| a.ci_id.cmp(&b.ci_id))
}

fn manifest_partition_priority(partition: Partition) -> u8 {
    match partition {
        P0Policy => 5,
        P1TaskFacts => 4,
        P2Evidence => 3,
        P3WorkingDelta => 2,
        P4Dialogue => 1,
    }
}

fn compute_manifest_digest(
    anchor: &Anchor,
    version: u32,
    working_generation: u32,
    segments: &[BundleSegment],
    budget: &BudgetSummary,
) -> Result<String, ContextError> {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "tenant:{}|envelope:{}|v{}|g{}|target{}|projected{}\n",
        anchor.tenant_id.into_inner(),
        anchor.envelope_id,
        version,
        working_generation,
        budget.target_tokens,
        budget.projected_tokens
    ));

    let mut flat: Vec<&BundleItem> = segments.iter().flat_map(|seg| seg.items.iter()).collect();
    flat.sort_by(|a, b| manifest_item_cmp(a, b));

    for item in flat {
        let digest = ensure_content_digest(item);
        hasher.update(format!(
            "{}|{}|{}|{}|{}|{}|{}\n",
            partition_key(item.partition),
            manifest_partition_priority(item.partition),
            item.score_scaled,
            item.ts_ms,
            item.tokens,
            item.ci_id,
            digest,
        ));
    }

    Ok(format!("man-{}", hex::encode(hasher.finalize())))
}

fn partition_key(partition: Partition) -> &'static str {
    match partition {
        P0Policy => "P0",
        P1TaskFacts => "P1",
        P2Evidence => "P2",
        P3WorkingDelta => "P3",
        P4Dialogue => "P4",
    }
}

fn ensure_content_digest(item: &BundleItem) -> String {
    if let Some(digest) = &item.digests.content {
        return digest.clone();
    }
    let mut hasher = Sha256::new();
    hasher.update(item.ci_id.as_bytes());
    hasher.update(item.tokens.to_le_bytes());
    hasher.update(item.score_scaled.to_le_bytes());
    for (key, value) in &item.score_stats.factors {
        hasher.update(key.as_bytes());
        hasher.update(value.to_le_bytes());
    }
    if let Some(typ) = &item.typ {
        hasher.update(typ.as_bytes());
    }
    if let Some(supersedes) = &item.supersedes {
        hasher.update(supersedes.as_bytes());
    }
    for ptr in &item.evidence_ptrs {
        if let Ok(serialized) = serde_json::to_vec(ptr) {
            hasher.update(serialized);
        }
    }
    format!("cnt-{}", hex::encode(hasher.finalize()))
}

fn manifest_item_signature(item: &BundleItem) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        partition_key(item.partition),
        item.score_scaled,
        item.ts_ms,
        item.tokens,
        item.digests.content.as_deref().unwrap_or(""),
        item.digests.semantic.as_deref().unwrap_or(""),
        item.why_included.as_deref().unwrap_or(""),
    )
}

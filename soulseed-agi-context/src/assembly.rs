use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;
use soulseed_agi_core_models::DeltaPatch;
use xxhash_rust::xxh3::xxh3_64;

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

pub fn build_prefix(bundle: &ContextBundle) -> Result<ContextManifest, ContextError> {
    ensure_partitions(&bundle.segments)?;

    let mut segments = bundle.segments.clone();
    sort_segments(&mut segments);

    let manifest_digest = compute_manifest_digest(&bundle.anchor, 1, &segments, &bundle.budget)?;

    Ok(ContextManifest {
        anchor: bundle.anchor.clone(),
        version: 1,
        segments,
        budget: bundle.budget.clone(),
        manifest_digest,
    })
}

pub fn merge_delta(
    manifest: &ContextManifest,
    delta_bundle: &ContextBundle,
    patch: &DeltaPatch,
) -> Result<(ContextManifest, MergeReport), ContextError> {
    ensure_anchor_match(&manifest.anchor, &delta_bundle.anchor)?;

    let mut segments_map = segments_to_map(&manifest.segments);
    let delta_map = segments_to_item_map(&delta_bundle.segments);

    let mut added_applied = Vec::new();
    let mut updated_applied = Vec::new();
    let mut removed_applied = Vec::new();

    for id in &patch.removed {
        if remove_item(&mut segments_map, id).is_some() {
            removed_applied.push(id.clone());
        } else {
            return Err(ContextError::DeltaMismatch(format!("remove_missing:{id}")));
        }
    }

    for id in &patch.updated {
        let Some((partition, new_item)) = delta_map.get(id) else {
            return Err(ContextError::DeltaMismatch(format!("update_missing:{id}")));
        };
        let existing_partition = find_partition(&segments_map, id)
            .ok_or_else(|| ContextError::DeltaMismatch(format!("update_target_missing:{id}")))?;
        if existing_partition != *partition {
            remove_item(&mut segments_map, id);
        }
        upsert_item(&mut segments_map, *partition, new_item.clone());
        updated_applied.push(id.clone());
    }

    for id in &patch.added {
        if find_partition(&segments_map, id).is_some() {
            return Err(ContextError::DeltaMismatch(format!("add_conflict:{id}")));
        }
        let Some((partition, new_item)) = delta_map.get(id) else {
            return Err(ContextError::DeltaMismatch(format!("add_missing:{id}")));
        };
        upsert_item(&mut segments_map, *partition, new_item.clone());
        added_applied.push(id.clone());
    }

    let ignored = delta_map
        .keys()
        .filter(|id| {
            !patch.added.contains(id) && !patch.updated.contains(id) && !patch.removed.contains(id)
        })
        .cloned()
        .collect();

    let mut segments = map_to_segments(segments_map);
    sort_segments(&mut segments);

    let new_version = manifest.version.saturating_add(1);
    let manifest_digest = compute_manifest_digest(
        &manifest.anchor,
        new_version,
        &segments,
        &delta_bundle.budget,
    )?;

    let new_manifest = ContextManifest {
        anchor: manifest.anchor.clone(),
        version: new_version,
        segments,
        budget: delta_bundle.budget.clone(),
        manifest_digest,
    };

    let report = MergeReport {
        added: added_applied,
        updated: updated_applied,
        removed: removed_applied,
        ignored,
    };

    Ok((new_manifest, report))
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
    let manifest_digest =
        compute_manifest_digest(&manifest.anchor, new_version, &segments, &manifest.budget)
            .expect("manifest digest");

    let new_manifest = ContextManifest {
        anchor: manifest.anchor.clone(),
        version: new_version,
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

fn segments_to_item_map(segments: &[BundleSegment]) -> HashMap<String, (Partition, BundleItem)> {
    let mut map = HashMap::new();
    for seg in segments {
        for item in &seg.items {
            map.insert(item.ci_id.clone(), (seg.partition, item.clone()));
        }
    }
    map
}

fn map_to_segments(map: HashMap<Partition, Vec<BundleItem>>) -> Vec<BundleSegment> {
    map.into_iter()
        .map(|(partition, mut items)| {
            items.sort_by(|a, b| a.ci_id.cmp(&b.ci_id));
            BundleSegment { partition, items }
        })
        .collect()
}

fn remove_item(map: &mut HashMap<Partition, Vec<BundleItem>>, id: &str) -> Option<BundleItem> {
    for items in map.values_mut() {
        if let Some(pos) = items.iter().position(|item| item.ci_id == id) {
            return Some(items.remove(pos));
        }
    }
    None
}

fn upsert_item(
    map: &mut HashMap<Partition, Vec<BundleItem>>,
    partition: Partition,
    item: BundleItem,
) {
    map.entry(partition).or_default();
    let items = map.get_mut(&partition).expect("entry exists");
    if let Some(pos) = items
        .iter()
        .position(|existing| existing.ci_id == item.ci_id)
    {
        items[pos] = item;
    } else {
        items.push(item);
    }
}

fn find_partition(map: &HashMap<Partition, Vec<BundleItem>>, id: &str) -> Option<Partition> {
    for (partition, items) in map.iter() {
        if items.iter().any(|item| item.ci_id == id) {
            return Some(*partition);
        }
    }
    None
}

fn sort_segments(segments: &mut Vec<BundleSegment>) {
    for seg in segments.iter_mut() {
        seg.items.sort_by(|a, b| a.ci_id.cmp(&b.ci_id));
    }
    segments.sort_by_key(|seg| partition_rank(seg.partition));
}

fn partition_rank(partition: Partition) -> u8 {
    match partition {
        P0Policy => 0,
        P1TaskFacts => 1,
        P2Evidence => 2,
        P3WorkingDelta => 3,
        P4Dialogue => 4,
    }
}

fn compute_manifest_digest(
    anchor: &Anchor,
    version: u32,
    segments: &[BundleSegment],
    budget: &BudgetSummary,
) -> Result<String, ContextError> {
    let manifest_json = json!({
        "tenant_id": anchor.tenant_id.into_inner(),
        "envelope_id": anchor.envelope_id.to_string(),
        "config_snapshot_hash": anchor.config_snapshot_hash,
        "config_snapshot_version": anchor.config_snapshot_version,
        "session_id": anchor.session_id.map(|id| id.into_inner()),
        "sequence_number": anchor.sequence_number,
        "access_class": anchor.access_class,
        "provenance": anchor.provenance,
        "scenario": anchor.scenario,
        "schema_v": anchor.schema_v,
        "version": version,
        "budget": budget,
        "segments": segments,
    });

    let serialized = serde_json::to_vec(&manifest_json)
        .map_err(|err| ContextError::ManifestInvariant(format!("serde:{err}")))?;
    let digest = xxh3_64(&serialized);
    Ok(format!("man-{digest:016x}"))
}

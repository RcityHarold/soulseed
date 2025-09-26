use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest as ShaDigest, Sha256};

use crate::dto::EnvironmentContext;
use crate::errors::{EnvCtxError, Result};

pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).map_err(|err| EnvCtxError::Digest(err.to_string()))?;
    let canonical = canonicalize(value);
    serde_json::to_vec(&canonical).map_err(|err| EnvCtxError::Digest(err.to_string()))
}

pub fn compute_digest(ctx: &EnvironmentContext) -> Result<String> {
    let bytes = canonical_bytes(ctx)?;
    let sha = Sha256::digest(&bytes);
    let blake = blake3::hash(&bytes);
    Ok(format!(
        "sha256:{}|blake3:{}",
        hex::encode(sha),
        blake.to_hex()
    ))
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(canonicalize_object(map)),
        Value::Array(vec) => {
            let canon = vec.into_iter().map(canonicalize).collect();
            Value::Array(canon)
        }
        other => other,
    }
}

fn canonicalize_object(map: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut canonical = Map::with_capacity(entries.len());
    for (key, value) in entries {
        canonical.insert(key, canonicalize(value));
    }
    canonical
}

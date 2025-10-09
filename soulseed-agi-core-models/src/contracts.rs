pub const IDX_TENANT_FIRST: &str = "/* all primary indexes must start with tenant_id */";
pub const IDX_SESSION_ORDER_UNIQUE: &str = "UNIQUE (tenant_id, session_id, sequence_number)";
pub const IDX_TIMELINE: &str = "INDEX (tenant_id, timestamp_ms, event_id)";
pub const NO_TABLE_SCAN: &str = "/* queries must declare indices_used[]; otherwise reject */";
pub const IDX_VECTOR_SEARCH: &str =
    "VECTOR INDEX (tenant_id, embedding_meta.model, content_embedding)";
pub const IDX_APPEND_ONLY_CHAIN: &str =
    "INDEX (tenant_id, supersedes) /* append-only supersedes chain */";
pub const LIVE_SUBSCRIPTION_CONTRACT: &str =
    "/* live responses must include query_hash, indices_used, resp_bytes */";

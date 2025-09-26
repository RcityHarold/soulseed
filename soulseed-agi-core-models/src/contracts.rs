pub const IDX_TENANT_FIRST: &str = "/* all primary indexes must start with tenant_id */";
pub const IDX_SESSION_ORDER_UNIQUE: &str = "UNIQUE (tenant_id, session_id, sequence_number)";
pub const IDX_TIMELINE: &str = "INDEX (tenant_id, timestamp_ms, event_id)";
pub const NO_TABLE_SCAN: &str = "/* queries must declare indices_used[]; otherwise reject */";

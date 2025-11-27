-- SurrealDB initial schema for Soulseed data plane
-- Run manually: surreal sql --conn <endpoint> -f migrations/surreal/001_soulseed_init.sql

BEGIN TRANSACTION;

-- =====================
-- Dialogue Events
-- =====================
DEFINE TABLE dialogue_event SCHEMAFULL;
DEFINE FIELD tenant_id             ON dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD event_id              ON dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD session_id            ON dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD subject               ON dialogue_event TYPE object;
DEFINE FIELD participants          ON dialogue_event TYPE array;
DEFINE FIELD head                  ON dialogue_event TYPE object;
DEFINE FIELD snapshot              ON dialogue_event TYPE object;
DEFINE FIELD timestamp_ms          ON dialogue_event TYPE number;
DEFINE FIELD scenario              ON dialogue_event TYPE string;
DEFINE FIELD event_type            ON dialogue_event TYPE string;
DEFINE FIELD time_window           ON dialogue_event TYPE option<string>;
DEFINE FIELD access_class          ON dialogue_event TYPE string;
DEFINE FIELD provenance            ON dialogue_event TYPE option<object>;
DEFINE FIELD sequence_number       ON dialogue_event TYPE number;
DEFINE FIELD trigger_event_id      ON dialogue_event TYPE option<string>;
DEFINE FIELD temporal_pattern_id   ON dialogue_event TYPE option<string>;
DEFINE FIELD metadata              ON dialogue_event TYPE option<object>;
DEFINE FIELD content_embedding     ON dialogue_event TYPE option<array>;
DEFINE FIELD context_embedding     ON dialogue_event TYPE option<array>;
DEFINE FIELD decision_embedding    ON dialogue_event TYPE option<array>;
DEFINE FIELD embedding_meta        ON dialogue_event TYPE option<object>;
DEFINE FIELD concept_vector        ON dialogue_event TYPE option<array>;
DEFINE FIELD real_time_priority    ON dialogue_event TYPE option<string>;
DEFINE FIELD live_stream_id        ON dialogue_event TYPE option<string>;
DEFINE FIELD evidence_pointer      ON dialogue_event TYPE option<object>;
DEFINE FIELD content_digest_sha256 ON dialogue_event TYPE option<string>;
DEFINE FIELD blob_ref              ON dialogue_event TYPE option<string>;
DEFINE FIELD message_ref           ON dialogue_event TYPE option<object>;
DEFINE FIELD tool_invocation       ON dialogue_event TYPE option<object>;
DEFINE FIELD tool_result           ON dialogue_event TYPE option<object>;
DEFINE FIELD self_reflection       ON dialogue_event TYPE option<object>;

-- timeline: tenant + timestamp sorting
DEFINE INDEX idx_dialogue_event_timeline
    ON TABLE dialogue_event FIELDS tenant_id, timestamp_ms, event_id;

-- session ordered events (DC→AC 节律)
DEFINE INDEX idx_dialogue_event_session_order
    ON TABLE dialogue_event FIELDS tenant_id, session_id, sequence_number;

-- causal graph parent lookup
DEFINE INDEX idx_dialogue_event_causal
    ON TABLE dialogue_event FIELDS tenant_id, trigger_event_id;

-- semantic vector search (optional)
DEFINE INDEX idx_dialogue_event_vector
    ON TABLE dialogue_event FIELDS concept_vector
    HNSW DIMENSION 1536 M 16 EFC 200;

-- sparse BM25 fallback on textual metadata
DEFINE INDEX idx_dialogue_event_sparse
    ON TABLE dialogue_event FIELDS metadata
    SEARCH BM25;

-- ensure tenant scoped uniqueness
DEFINE INDEX idx_dialogue_event_event_uniq
    ON TABLE dialogue_event FIELDS tenant_id, event_id UNIQUE;

-- =====================
-- Awareness Events
-- =====================
DEFINE TABLE awareness_event SCHEMAFULL;
DEFINE FIELD tenant_id            ON awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD event_id             ON awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD awareness_cycle_id   ON awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD event_type           ON awareness_event TYPE string;
DEFINE FIELD occurred_at_ms       ON awareness_event TYPE number;
DEFINE FIELD parent_cycle_id      ON awareness_event TYPE option<string>;
DEFINE FIELD collab_scope_id      ON awareness_event TYPE option<string>;
DEFINE FIELD barrier_id           ON awareness_event TYPE option<string>;
DEFINE FIELD env_mode             ON awareness_event TYPE option<string>;
DEFINE FIELD inference_cycle_sequence ON awareness_event TYPE number;
DEFINE FIELD degradation_reason   ON awareness_event TYPE option<string>;
DEFINE FIELD payload              ON awareness_event TYPE object;
DEFINE FIELD anchor               ON awareness_event TYPE object;

DEFINE INDEX ix_awareness_cycle_time
    ON TABLE awareness_event FIELDS tenant_id, awareness_cycle_id, occurred_at_ms;

DEFINE INDEX ix_awareness_parent_cycle
    ON TABLE awareness_event FIELDS tenant_id, parent_cycle_id;

DEFINE INDEX ix_awareness_collab_scope
    ON TABLE awareness_event FIELDS tenant_id, collab_scope_id;

DEFINE INDEX ix_awareness_barrier
    ON TABLE awareness_event FIELDS tenant_id, barrier_id;

DEFINE INDEX ix_awareness_env_mode
    ON TABLE awareness_event FIELDS tenant_id, env_mode;

DEFINE INDEX ix_awareness_event_uniq
    ON TABLE awareness_event FIELDS tenant_id, event_id UNIQUE;

-- =====================
-- Context Assembly artefacts
-- =====================
DEFINE TABLE context_item SCHEMAFULL;
DEFINE FIELD tenant_id       ON context_item TYPE string ASSERT $value != "";
DEFINE FIELD anchor          ON context_item TYPE object;
DEFINE FIELD context_id      ON context_item TYPE string ASSERT $value != "";
DEFINE FIELD partition_hint  ON context_item TYPE option<string>;
DEFINE FIELD source_event_id ON context_item TYPE string;
DEFINE FIELD source_message_id ON context_item TYPE option<string>;
DEFINE FIELD content         ON context_item TYPE object;
DEFINE FIELD tokens          ON context_item TYPE number;
DEFINE FIELD features        ON context_item TYPE object;
DEFINE FIELD policy_tags     ON context_item TYPE object;
DEFINE FIELD evidence        ON context_item TYPE option<object>;

DEFINE INDEX idx_context_item_anchor
    ON TABLE context_item FIELDS tenant_id, anchor.envelope_id, context_id;

DEFINE INDEX idx_context_item_partition
    ON TABLE context_item FIELDS tenant_id, partition_hint;

DEFINE TABLE context_manifest SCHEMAFULL;
DEFINE FIELD tenant_id      ON context_manifest TYPE string ASSERT $value != "";
DEFINE FIELD manifest_id    ON context_manifest TYPE string ASSERT $value != "";
DEFINE FIELD anchor         ON context_manifest TYPE object;
DEFINE FIELD version        ON context_manifest TYPE number;
DEFINE FIELD manifest_digest ON context_manifest TYPE string;
DEFINE FIELD segments       ON context_manifest TYPE array;
DEFINE FIELD explain        ON context_manifest TYPE object;
DEFINE FIELD created_at_ms  ON context_manifest TYPE number;

DEFINE INDEX idx_context_manifest_anchor
    ON TABLE context_manifest FIELDS tenant_id, anchor.envelope_id, version;

DEFINE INDEX idx_context_manifest_digest
    ON TABLE context_manifest FIELDS tenant_id, manifest_digest UNIQUE;

-- =====================
-- ACE Persistence
-- =====================
-- 注意：使用 SCHEMALESS 以支持灵活的 JSON 对象存储
-- payload 和 metadata 包含复杂的枚举和嵌套结构，不适合严格的类型定义
DEFINE TABLE ace_dialogue_event SCHEMALESS;
DEFINE FIELD tenant             ON ace_dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD event_id           ON ace_dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD cycle_id           ON ace_dialogue_event TYPE string ASSERT $value != "";
DEFINE FIELD occurred_at        ON ace_dialogue_event TYPE number;
DEFINE FIELD lane               ON ace_dialogue_event TYPE string;
-- payload 和 metadata 字段不定义类型，允许任意 JSON 结构
DEFINE FIELD manifest_digest    ON ace_dialogue_event TYPE option<string>;
DEFINE FIELD explain_fingerprint ON ace_dialogue_event TYPE option<string>;
DEFINE FIELD created_at         ON ace_dialogue_event TYPE number;

DEFINE INDEX idx_ace_dialogue_event_lookup
    ON TABLE ace_dialogue_event FIELDS tenant, event_id UNIQUE;

DEFINE INDEX idx_ace_dialogue_event_cycle
    ON TABLE ace_dialogue_event FIELDS tenant, cycle_id;

DEFINE INDEX idx_ace_dialogue_event_created
    ON TABLE ace_dialogue_event FIELDS tenant, created_at;

-- ACE Awareness Events 表
-- SCHEMALESS 模式允许动态字段如 event_data，payload 等
DEFINE TABLE ace_awareness_event SCHEMALESS;
DEFINE FIELD tenant             ON ace_awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD event_id           ON ace_awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD cycle_id           ON ace_awareness_event TYPE string ASSERT $value != "";
DEFINE FIELD event_type         ON ace_awareness_event TYPE string;
DEFINE FIELD occurred_at        ON ace_awareness_event TYPE number;
DEFINE FIELD parent_cycle_id    ON ace_awareness_event TYPE option<string>;
DEFINE FIELD collab_scope_id    ON ace_awareness_event TYPE option<string>;
-- event_data 字段在代码中动态填充，不需要在 SCHEMALESS 表中显式定义
DEFINE FIELD created_at         ON ace_awareness_event TYPE number;

DEFINE INDEX idx_ace_awareness_event_lookup
    ON TABLE ace_awareness_event FIELDS tenant, event_id UNIQUE;

DEFINE INDEX idx_ace_awareness_event_cycle
    ON TABLE ace_awareness_event FIELDS tenant, cycle_id;

DEFINE INDEX idx_ace_awareness_event_type
    ON TABLE ace_awareness_event FIELDS tenant, event_type;

DEFINE TABLE ace_cycle_snapshot SCHEMALESS;
DEFINE FIELD tenant ON ace_cycle_snapshot TYPE string ASSERT $value != "";
DEFINE FIELD cycle_id ON ace_cycle_snapshot TYPE string ASSERT $value != "";
DEFINE FIELD snapshot_data ON ace_cycle_snapshot FLEXIBLE;
DEFINE FIELD created_at ON ace_cycle_snapshot TYPE number;
DEFINE FIELD updated_at ON ace_cycle_snapshot TYPE number;

DEFINE INDEX idx_ace_cycle_snapshot_lookup
    ON TABLE ace_cycle_snapshot FIELDS tenant, cycle_id UNIQUE;

DEFINE INDEX idx_ace_cycle_snapshot_created
    ON TABLE ace_cycle_snapshot FIELDS tenant, created_at;

-- =====================
-- Session Table (场景五自主延续执行模式)
-- =====================
DEFINE TABLE session SCHEMALESS;
DEFINE FIELD tenant_id            ON session TYPE string ASSERT $value != "";
DEFINE FIELD session_id           ON session TYPE string ASSERT $value != "";
DEFINE FIELD trace_id             ON session TYPE string;
DEFINE FIELD correlation_id       ON session TYPE string;
DEFINE FIELD subject              ON session TYPE object;
DEFINE FIELD participants         ON session TYPE array;
DEFINE FIELD head                 ON session TYPE object;
DEFINE FIELD snapshot             ON session TYPE object;
DEFINE FIELD created_at           ON session TYPE number;
DEFINE FIELD scenario             ON session TYPE option<string>;
DEFINE FIELD access_class         ON session TYPE string;
DEFINE FIELD provenance           ON session TYPE option<object>;
DEFINE FIELD supersedes           ON session TYPE option<string>;
DEFINE FIELD superseded_by        ON session TYPE option<string>;
DEFINE FIELD evidence_pointer     ON session TYPE option<object>;
DEFINE FIELD blob_ref             ON session TYPE option<string>;
DEFINE FIELD content_digest_sha256 ON session TYPE option<string>;
DEFINE FIELD metadata             ON session TYPE option<object>;
-- 场景五自主延续执行模式字段
DEFINE FIELD mode                 ON session TYPE string DEFAULT 'interactive';
DEFINE FIELD orchestration_id     ON session TYPE option<string>;
DEFINE FIELD agenda_queue         ON session TYPE array DEFAULT [];
DEFINE FIELD continuation_config  ON session TYPE option<object>;
DEFINE FIELD consecutive_ac_count ON session TYPE number DEFAULT 0;
DEFINE FIELD consecutive_idle_count ON session TYPE number DEFAULT 0;
DEFINE FIELD total_cost_spent     ON session TYPE number DEFAULT 0;
DEFINE FIELD last_activity_at_ms  ON session TYPE option<number>;
DEFINE FIELD scenario_stack       ON session TYPE array DEFAULT [];

DEFINE INDEX idx_session_tenant_id
    ON TABLE session FIELDS tenant_id, session_id UNIQUE;

DEFINE INDEX idx_session_orchestration
    ON TABLE session FIELDS tenant_id, orchestration_id;

DEFINE INDEX idx_session_mode
    ON TABLE session FIELDS tenant_id, mode;

-- =====================
-- Agenda Item Table (议程项)
-- =====================
DEFINE TABLE agenda_item SCHEMAFULL;
DEFINE FIELD tenant_id            ON agenda_item TYPE string ASSERT $value != "";
DEFINE FIELD session_id           ON agenda_item TYPE string ASSERT $value != "";
DEFINE FIELD item_id              ON agenda_item TYPE string ASSERT $value != "";
DEFINE FIELD description          ON agenda_item TYPE string;
DEFINE FIELD priority             ON agenda_item TYPE number DEFAULT 100;
DEFINE FIELD status               ON agenda_item TYPE string DEFAULT 'pending';
DEFINE FIELD created_at           ON agenda_item TYPE datetime;
DEFINE FIELD estimated_completion ON agenda_item TYPE option<datetime>;
DEFINE FIELD completed_at         ON agenda_item TYPE option<datetime>;
DEFINE FIELD assigned_ac_id       ON agenda_item TYPE option<number>;
DEFINE FIELD metadata             ON agenda_item TYPE option<object>;

DEFINE INDEX idx_agenda_item_lookup
    ON TABLE agenda_item FIELDS tenant_id, session_id, item_id UNIQUE;

DEFINE INDEX idx_agenda_item_status
    ON TABLE agenda_item FIELDS tenant_id, session_id, status;

DEFINE INDEX idx_agenda_item_priority
    ON TABLE agenda_item FIELDS tenant_id, session_id, priority;

-- =====================
-- Version Chain Table (版本链管理)
-- =====================
DEFINE TABLE version_chain_entry SCHEMAFULL;
DEFINE FIELD tenant_id        ON version_chain_entry TYPE string ASSERT $value != "";
DEFINE FIELD entity_id        ON version_chain_entry TYPE string ASSERT $value != "";
DEFINE FIELD entity_type      ON version_chain_entry TYPE string;  -- dialogue_event, session, message, artifact
DEFINE FIELD supersedes       ON version_chain_entry TYPE option<string>;
DEFINE FIELD superseded_by    ON version_chain_entry TYPE option<string>;
DEFINE FIELD version          ON version_chain_entry TYPE number DEFAULT 1;
DEFINE FIELD created_at_ms    ON version_chain_entry TYPE number;
DEFINE FIELD chain_id         ON version_chain_entry TYPE string;
DEFINE FIELD is_current       ON version_chain_entry TYPE bool DEFAULT true;
DEFINE FIELD description      ON version_chain_entry TYPE option<string>;

DEFINE INDEX idx_version_chain_lookup
    ON TABLE version_chain_entry FIELDS tenant_id, entity_id UNIQUE;

DEFINE INDEX idx_version_chain_chain
    ON TABLE version_chain_entry FIELDS tenant_id, chain_id;

DEFINE INDEX idx_version_chain_supersedes
    ON TABLE version_chain_entry FIELDS tenant_id, supersedes;

DEFINE INDEX idx_version_chain_current
    ON TABLE version_chain_entry FIELDS tenant_id, entity_type, is_current;

-- 版本链关系边（用于图遍历）
DEFINE TABLE supersedes SCHEMAFULL;
DEFINE FIELD in           ON supersedes TYPE record<version_chain_entry>;
DEFINE FIELD out          ON supersedes TYPE record<version_chain_entry>;
DEFINE FIELD created_at_ms ON supersedes TYPE number;

-- =====================
-- Outbox & Replay (ACE → Soulbase)
-- =====================
DEFINE TABLE outbox_envelope SCHEMAFULL;
DEFINE FIELD tenant_id    ON outbox_envelope TYPE string ASSERT $value != "";
DEFINE FIELD cycle_id     ON outbox_envelope TYPE string ASSERT $value != "";
DEFINE FIELD state        ON outbox_envelope TYPE string;
DEFINE FIELD created_at_ms ON outbox_envelope TYPE number;
DEFINE FIELD updated_at_ms ON outbox_envelope TYPE number;
DEFINE FIELD messages     ON outbox_envelope TYPE array;

DEFINE INDEX idx_outbox_tenant_cycle
    ON TABLE outbox_envelope FIELDS tenant_id, cycle_id UNIQUE;

DEFINE INDEX idx_outbox_state
    ON TABLE outbox_envelope FIELDS tenant_id, state;

DEFINE TABLE replay_cursor SCHEMAFULL;
DEFINE FIELD tenant_id     ON replay_cursor TYPE string ASSERT $value != "";
DEFINE FIELD stream_kind   ON replay_cursor TYPE string;
DEFINE FIELD last_event_id ON replay_cursor TYPE string;
DEFINE FIELD updated_at_ms ON replay_cursor TYPE number;

DEFINE INDEX idx_replay_cursor_unique
    ON TABLE replay_cursor FIELDS tenant_id, stream_kind UNIQUE;

COMMIT TRANSACTION;

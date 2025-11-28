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

-- =====================
-- Graph Nodes Schema (六大核心节点)
-- 用于 SurrealDB 图谱存储
-- =====================

-- Actor 节点表
DEFINE TABLE graph_actor SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_actor TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_actor TYPE string ASSERT $value != "";
DEFINE FIELD kind ON graph_actor TYPE string;
DEFINE FIELD internal_id ON graph_actor TYPE number;
DEFINE FIELD display_name ON graph_actor TYPE option<string>;
DEFINE FIELD role ON graph_actor TYPE option<string>;
DEFINE FIELD capabilities ON graph_actor TYPE array DEFAULT [];
DEFINE FIELD status ON graph_actor TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON graph_actor TYPE number;
DEFINE FIELD last_active_at_ms ON graph_actor TYPE option<number>;
DEFINE FIELD metadata ON graph_actor FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_actor_lookup ON TABLE graph_actor FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_actor_kind ON TABLE graph_actor FIELDS tenant_id, kind;

-- Event 节点表
DEFINE TABLE graph_event SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_event TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_event TYPE string ASSERT $value != "";
DEFINE FIELD event_type ON graph_event TYPE string;
DEFINE FIELD session_id ON graph_event TYPE string;
DEFINE FIELD ac_id ON graph_event TYPE option<number>;
DEFINE FIELD actor_id ON graph_event TYPE string;
DEFINE FIELD occurred_at_ms ON graph_event TYPE number;
DEFINE FIELD sequence_number ON graph_event TYPE number DEFAULT 0;
DEFINE FIELD scenario ON graph_event TYPE option<string>;
DEFINE FIELD trigger_event_id ON graph_event TYPE option<string>;
DEFINE FIELD access_class ON graph_event TYPE string DEFAULT 'internal';
DEFINE FIELD payload_digest ON graph_event TYPE option<string>;
DEFINE FIELD metadata ON graph_event FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_event_lookup ON TABLE graph_event FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_event_session ON TABLE graph_event FIELDS tenant_id, session_id, occurred_at_ms;
DEFINE INDEX idx_graph_event_type ON TABLE graph_event FIELDS tenant_id, event_type;
DEFINE INDEX idx_graph_event_actor ON TABLE graph_event FIELDS tenant_id, actor_id;

-- Message 节点表
DEFINE TABLE graph_message SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_message TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_message TYPE string ASSERT $value != "";
DEFINE FIELD event_id ON graph_message TYPE string;
DEFINE FIELD session_id ON graph_message TYPE string;
DEFINE FIELD sender_id ON graph_message TYPE string;
DEFINE FIELD content_type ON graph_message TYPE string DEFAULT 'text';
DEFINE FIELD content_summary ON graph_message TYPE option<string>;
DEFINE FIELD content_digest ON graph_message TYPE option<string>;
DEFINE FIELD token_count ON graph_message TYPE option<number>;
DEFINE FIELD language ON graph_message TYPE option<string>;
DEFINE FIELD sentiment ON graph_message TYPE option<string>;
DEFINE FIELD is_final ON graph_message TYPE bool DEFAULT true;
DEFINE FIELD reply_to ON graph_message TYPE option<string>;
DEFINE FIELD created_at_ms ON graph_message TYPE number;
DEFINE FIELD edited_at_ms ON graph_message TYPE option<number>;
DEFINE FIELD metadata ON graph_message FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_message_lookup ON TABLE graph_message FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_message_session ON TABLE graph_message FIELDS tenant_id, session_id, created_at_ms;
DEFINE INDEX idx_graph_message_sender ON TABLE graph_message FIELDS tenant_id, sender_id;

-- Session 节点表
DEFINE TABLE graph_session SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_session TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_session TYPE string ASSERT $value != "";
DEFINE FIELD mode ON graph_session TYPE string DEFAULT 'interactive';
DEFINE FIELD scenario ON graph_session TYPE option<string>;
DEFINE FIELD creator_id ON graph_session TYPE string;
DEFINE FIELD participant_ids ON graph_session TYPE array DEFAULT [];
DEFINE FIELD status ON graph_session TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON graph_session TYPE number;
DEFINE FIELD last_activity_at_ms ON graph_session TYPE option<number>;
DEFINE FIELD ended_at_ms ON graph_session TYPE option<number>;
DEFINE FIELD event_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD message_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD ac_count ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD scenario_stack_depth ON graph_session TYPE number DEFAULT 0;
DEFINE FIELD metadata ON graph_session FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_session_lookup ON TABLE graph_session FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_session_status ON TABLE graph_session FIELDS tenant_id, status;
DEFINE INDEX idx_graph_session_creator ON TABLE graph_session FIELDS tenant_id, creator_id;

-- AC 节点表
DEFINE TABLE graph_ac SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_ac TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_ac TYPE string ASSERT $value != "";
DEFINE FIELD session_id ON graph_ac TYPE string;
DEFINE FIELD parent_ac_id ON graph_ac TYPE option<string>;
DEFINE FIELD lane ON graph_ac TYPE string;
DEFINE FIELD ic_count ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD status ON graph_ac TYPE string DEFAULT 'pending';
DEFINE FIELD started_at_ms ON graph_ac TYPE number;
DEFINE FIELD ended_at_ms ON graph_ac TYPE option<number>;
DEFINE FIELD duration_ms ON graph_ac TYPE option<number>;
DEFINE FIELD tokens_spent ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD external_cost ON graph_ac TYPE number DEFAULT 0;
DEFINE FIELD decision_confidence ON graph_ac TYPE option<number>;
DEFINE FIELD collab_scope_id ON graph_ac TYPE option<string>;
DEFINE FIELD trigger_event_id ON graph_ac TYPE option<string>;
DEFINE FIELD final_event_id ON graph_ac TYPE option<string>;
DEFINE FIELD metadata ON graph_ac FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_ac_lookup ON TABLE graph_ac FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_ac_session ON TABLE graph_ac FIELDS tenant_id, session_id;
DEFINE INDEX idx_graph_ac_lane ON TABLE graph_ac FIELDS tenant_id, lane;
DEFINE INDEX idx_graph_ac_status ON TABLE graph_ac FIELDS tenant_id, status;
DEFINE INDEX idx_graph_ac_parent ON TABLE graph_ac FIELDS tenant_id, parent_ac_id;

-- Artifact 节点表
DEFINE TABLE graph_artifact SCHEMAFULL;
DEFINE FIELD tenant_id ON graph_artifact TYPE number ASSERT $value > 0;
DEFINE FIELD node_id ON graph_artifact TYPE string ASSERT $value != "";
DEFINE FIELD kind ON graph_artifact TYPE string;
DEFINE FIELD title ON graph_artifact TYPE string;
DEFINE FIELD description ON graph_artifact TYPE option<string>;
DEFINE FIELD source_event_id ON graph_artifact TYPE string;
DEFINE FIELD session_id ON graph_artifact TYPE string;
DEFINE FIELD creator_id ON graph_artifact TYPE string;
DEFINE FIELD content_digest ON graph_artifact TYPE option<string>;
DEFINE FIELD blob_ref ON graph_artifact TYPE option<string>;
DEFINE FIELD size_bytes ON graph_artifact TYPE option<number>;
DEFINE FIELD mime_type ON graph_artifact TYPE option<string>;
DEFINE FIELD version ON graph_artifact TYPE number DEFAULT 1;
DEFINE FIELD supersedes_id ON graph_artifact TYPE option<string>;
DEFINE FIELD created_at_ms ON graph_artifact TYPE number;
DEFINE FIELD updated_at_ms ON graph_artifact TYPE option<number>;
DEFINE FIELD tags ON graph_artifact TYPE array DEFAULT [];
DEFINE FIELD metadata ON graph_artifact FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_graph_artifact_lookup ON TABLE graph_artifact FIELDS tenant_id, node_id UNIQUE;
DEFINE INDEX idx_graph_artifact_session ON TABLE graph_artifact FIELDS tenant_id, session_id;
DEFINE INDEX idx_graph_artifact_kind ON TABLE graph_artifact FIELDS tenant_id, kind;
DEFINE INDEX idx_graph_artifact_creator ON TABLE graph_artifact FIELDS tenant_id, creator_id;

-- =====================
-- Graph Edges Schema (七大关系边族群)
-- =====================

-- 1. 因果边族群

-- triggered_by 边 (event -> event)
DEFINE TABLE triggered_by SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON triggered_by TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON triggered_by TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON triggered_by TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON triggered_by TYPE option<number>;
DEFINE FIELD confidence ON triggered_by TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON triggered_by TYPE number;
DEFINE FIELD metadata ON triggered_by FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_triggered_by_tenant ON TABLE triggered_by FIELDS tenant_id;

-- caused_by 边 (event -> event)
DEFINE TABLE caused_by SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON caused_by TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON caused_by TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON caused_by TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON caused_by TYPE option<number>;
DEFINE FIELD confidence ON caused_by TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON caused_by TYPE number;
DEFINE FIELD metadata ON caused_by FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_caused_by_tenant ON TABLE caused_by FIELDS tenant_id;

-- enabled 边 (event -> event)
DEFINE TABLE enabled SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON enabled TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON enabled TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON enabled TYPE number DEFAULT 1;
DEFINE FIELD created_at_ms ON enabled TYPE number;
DEFINE FIELD metadata ON enabled FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_enabled_tenant ON TABLE enabled FIELDS tenant_id;

-- led_to 边 (event -> event)
DEFINE TABLE led_to SCHEMAFULL TYPE RELATION FROM graph_event TO graph_event;
DEFINE FIELD tenant_id ON led_to TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON led_to TYPE number DEFAULT 1.0;
DEFINE FIELD depth ON led_to TYPE number DEFAULT 1;
DEFINE FIELD delay_ms ON led_to TYPE option<number>;
DEFINE FIELD confidence ON led_to TYPE number DEFAULT 1.0;
DEFINE FIELD created_at_ms ON led_to TYPE number;
DEFINE FIELD metadata ON led_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_led_to_tenant ON TABLE led_to FIELDS tenant_id;

-- 2. 归属边族群

-- belongs_to 边 (通用归属)
DEFINE TABLE belongs_to SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON belongs_to TYPE number ASSERT $value > 0;
DEFINE FIELD role ON belongs_to TYPE option<string>;
DEFINE FIELD order_index ON belongs_to TYPE option<number>;
DEFINE FIELD is_primary ON belongs_to TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON belongs_to TYPE number;
DEFINE FIELD metadata ON belongs_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_belongs_to_tenant ON TABLE belongs_to FIELDS tenant_id;

-- part_of 边 (部分-整体)
DEFINE TABLE part_of SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON part_of TYPE number ASSERT $value > 0;
DEFINE FIELD role ON part_of TYPE option<string>;
DEFINE FIELD order_index ON part_of TYPE option<number>;
DEFINE FIELD is_primary ON part_of TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON part_of TYPE number;
DEFINE FIELD metadata ON part_of FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_part_of_tenant ON TABLE part_of FIELDS tenant_id;

-- member_of 边 (成员-组)
DEFINE TABLE member_of SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON member_of TYPE number ASSERT $value > 0;
DEFINE FIELD role ON member_of TYPE option<string>;
DEFINE FIELD joined_at_ms ON member_of TYPE number;
DEFINE FIELD created_at_ms ON member_of TYPE number;
DEFINE FIELD metadata ON member_of FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_member_of_tenant ON TABLE member_of FIELDS tenant_id;

-- 3. 引用边族群

-- references_edge 边 (通用引用) - 避免与 SQL 关键字冲突
DEFINE TABLE references_edge SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON references_edge TYPE number ASSERT $value > 0;
DEFINE FIELD context ON references_edge TYPE option<string>;
DEFINE FIELD offset ON references_edge TYPE option<number>;
DEFINE FIELD length ON references_edge TYPE option<number>;
DEFINE FIELD reason ON references_edge TYPE option<string>;
DEFINE FIELD created_at_ms ON references_edge TYPE number;
DEFINE FIELD metadata ON references_edge FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_references_edge_tenant ON TABLE references_edge FIELDS tenant_id;

-- mentions 边 (message -> entity)
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION FROM graph_message;
DEFINE FIELD tenant_id ON mentions TYPE number ASSERT $value > 0;
DEFINE FIELD context ON mentions TYPE option<string>;
DEFINE FIELD offset ON mentions TYPE option<number>;
DEFINE FIELD length ON mentions TYPE option<number>;
DEFINE FIELD created_at_ms ON mentions TYPE number;
DEFINE FIELD metadata ON mentions FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_mentions_tenant ON TABLE mentions FIELDS tenant_id;

-- responds_to 边 (message -> message)
DEFINE TABLE responds_to SCHEMAFULL TYPE RELATION FROM graph_message TO graph_message;
DEFINE FIELD tenant_id ON responds_to TYPE number ASSERT $value > 0;
DEFINE FIELD reason ON responds_to TYPE option<string>;
DEFINE FIELD created_at_ms ON responds_to TYPE number;
DEFINE FIELD metadata ON responds_to FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_responds_to_tenant ON TABLE responds_to FIELDS tenant_id;

-- 4. 协作边族群

-- collaborates_with 边 (actor <-> actor)
DEFINE TABLE collaborates_with SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON collaborates_with TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON collaborates_with TYPE option<string>;
DEFINE FIELD session_id ON collaborates_with TYPE option<string>;
DEFINE FIELD started_at_ms ON collaborates_with TYPE number;
DEFINE FIELD ended_at_ms ON collaborates_with TYPE option<number>;
DEFINE FIELD status ON collaborates_with TYPE string DEFAULT 'active';
DEFINE FIELD collaboration_type ON collaborates_with TYPE option<string>;
DEFINE FIELD created_at_ms ON collaborates_with TYPE number;
DEFINE FIELD metadata ON collaborates_with FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_collaborates_tenant ON TABLE collaborates_with FIELDS tenant_id;
DEFINE INDEX idx_collaborates_status ON TABLE collaborates_with FIELDS tenant_id, status;

-- requests_help_from 边 (actor -> actor)
DEFINE TABLE requests_help_from SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON requests_help_from TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON requests_help_from TYPE option<string>;
DEFINE FIELD session_id ON requests_help_from TYPE option<string>;
DEFINE FIELD started_at_ms ON requests_help_from TYPE number;
DEFINE FIELD ended_at_ms ON requests_help_from TYPE option<number>;
DEFINE FIELD status ON requests_help_from TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON requests_help_from TYPE number;
DEFINE FIELD metadata ON requests_help_from FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_requests_help_tenant ON TABLE requests_help_from FIELDS tenant_id;

-- assists 边 (actor -> actor)
DEFINE TABLE assists SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_actor;
DEFINE FIELD tenant_id ON assists TYPE number ASSERT $value > 0;
DEFINE FIELD scope_id ON assists TYPE option<string>;
DEFINE FIELD session_id ON assists TYPE option<string>;
DEFINE FIELD started_at_ms ON assists TYPE number;
DEFINE FIELD ended_at_ms ON assists TYPE option<number>;
DEFINE FIELD status ON assists TYPE string DEFAULT 'active';
DEFINE FIELD created_at_ms ON assists TYPE number;
DEFINE FIELD metadata ON assists FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_assists_tenant ON TABLE assists FIELDS tenant_id;

-- 5. 版本边族群 (supersedes 已在上面定义为 version_chain 的关系边)

-- derived_from 边 (artifact -> artifact)
DEFINE TABLE derived_from SCHEMAFULL TYPE RELATION FROM graph_artifact TO graph_artifact;
DEFINE FIELD tenant_id ON derived_from TYPE number ASSERT $value > 0;
DEFINE FIELD from_version ON derived_from TYPE option<number>;
DEFINE FIELD to_version ON derived_from TYPE option<number>;
DEFINE FIELD change_description ON derived_from TYPE option<string>;
DEFINE FIELD is_main_line ON derived_from TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON derived_from TYPE number;
DEFINE FIELD metadata ON derived_from FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_derived_from_tenant ON TABLE derived_from FIELDS tenant_id;

-- evolved_into 边 (artifact -> artifact)
DEFINE TABLE evolved_into SCHEMAFULL TYPE RELATION FROM graph_artifact TO graph_artifact;
DEFINE FIELD tenant_id ON evolved_into TYPE number ASSERT $value > 0;
DEFINE FIELD from_version ON evolved_into TYPE option<number>;
DEFINE FIELD to_version ON evolved_into TYPE option<number>;
DEFINE FIELD change_description ON evolved_into TYPE option<string>;
DEFINE FIELD is_main_line ON evolved_into TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON evolved_into TYPE number;
DEFINE FIELD metadata ON evolved_into FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_evolved_into_tenant ON TABLE evolved_into FIELDS tenant_id;

-- 6. 影响边族群

-- influences 边 (entity -> entity)
DEFINE TABLE influences SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON influences TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON influences TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON influences TYPE string DEFAULT 'neutral';
DEFINE FIELD scope ON influences TYPE option<string>;
DEFINE FIELD is_persistent ON influences TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON influences TYPE number;
DEFINE FIELD metadata ON influences FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_influences_tenant ON TABLE influences FIELDS tenant_id;

-- impacts 边 (event -> state)
DEFINE TABLE impacts SCHEMAFULL TYPE RELATION FROM graph_event;
DEFINE FIELD tenant_id ON impacts TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON impacts TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON impacts TYPE string DEFAULT 'neutral';
DEFINE FIELD scope ON impacts TYPE option<string>;
DEFINE FIELD is_persistent ON impacts TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON impacts TYPE number;
DEFINE FIELD metadata ON impacts FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_impacts_tenant ON TABLE impacts FIELDS tenant_id;

-- affects 边 (action -> entity)
DEFINE TABLE affects SCHEMAFULL TYPE RELATION;
DEFINE FIELD tenant_id ON affects TYPE number ASSERT $value > 0;
DEFINE FIELD strength ON affects TYPE number DEFAULT 0.5;
DEFINE FIELD polarity ON affects TYPE string DEFAULT 'neutral';
DEFINE FIELD is_persistent ON affects TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON affects TYPE number;
DEFINE FIELD metadata ON affects FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_affects_tenant ON TABLE affects FIELDS tenant_id;

-- 7. 评价边族群

-- evaluates 边 (actor -> entity)
DEFINE TABLE evaluates SCHEMAFULL TYPE RELATION FROM graph_actor;
DEFINE FIELD tenant_id ON evaluates TYPE number ASSERT $value > 0;
DEFINE FIELD score ON evaluates TYPE option<number>;
DEFINE FIELD category ON evaluates TYPE option<string>;
DEFINE FIELD rationale ON evaluates TYPE option<string>;
DEFINE FIELD dimensions ON evaluates TYPE array DEFAULT [];
DEFINE FIELD is_formal ON evaluates TYPE bool DEFAULT false;
DEFINE FIELD created_at_ms ON evaluates TYPE number;
DEFINE FIELD metadata ON evaluates FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_evaluates_tenant ON TABLE evaluates FIELDS tenant_id;

-- rates 边 (actor -> artifact)
DEFINE TABLE rates SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_artifact;
DEFINE FIELD tenant_id ON rates TYPE number ASSERT $value > 0;
DEFINE FIELD score ON rates TYPE number ASSERT $value >= 0 AND $value <= 100;
DEFINE FIELD category ON rates TYPE option<string>;
DEFINE FIELD created_at_ms ON rates TYPE number;
DEFINE FIELD metadata ON rates FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_rates_tenant ON TABLE rates FIELDS tenant_id;

-- critiques 边 (actor -> artifact)
DEFINE TABLE critiques SCHEMAFULL TYPE RELATION FROM graph_actor TO graph_artifact;
DEFINE FIELD tenant_id ON critiques TYPE number ASSERT $value > 0;
DEFINE FIELD score ON critiques TYPE option<number>;
DEFINE FIELD rationale ON critiques TYPE option<string>;
DEFINE FIELD dimensions ON critiques TYPE array DEFAULT [];
DEFINE FIELD is_formal ON critiques TYPE bool DEFAULT true;
DEFINE FIELD created_at_ms ON critiques TYPE number;
DEFINE FIELD metadata ON critiques FLEXIBLE TYPE option<object>;

DEFINE INDEX idx_critiques_tenant ON TABLE critiques FIELDS tenant_id;

COMMIT TRANSACTION;

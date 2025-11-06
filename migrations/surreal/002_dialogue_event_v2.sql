-- 002_dialogue_event_v2.sql
-- Stage 4: 对齐 DialogueEvent 三层结构，新增 V2 表与索引。
-- 该脚本为草案，推荐在灰度环境中验证后再执行。

BEGIN;

-- 1. 新版事件主表（Append-Only）
DEFINE TABLE dialogue_event_v2
    SCHEMAFULL
    PERMISSIONS FULL
    COMMENT "DialogueEvent v2 主表（append-only）";

DEFINE FIELD id                      ON dialogue_event_v2 TYPE record<event>;
DEFINE FIELD tenant_id               ON dialogue_event_v2 TYPE string;
DEFINE FIELD session_id              ON dialogue_event_v2 TYPE string;
DEFINE FIELD event_id                ON dialogue_event_v2 TYPE string ASSERT string::len($value) > 0;
DEFINE FIELD scenario                ON dialogue_event_v2 TYPE string ASSERT string::len($value) > 0;
DEFINE FIELD event_type              ON dialogue_event_v2 TYPE string ASSERT string::len($value) > 0;
DEFINE FIELD sequence_number         ON dialogue_event_v2 TYPE number ASSERT $value >= 1;
DEFINE FIELD timestamp_ms            ON dialogue_event_v2 TYPE number ASSERT $value >= 0;
DEFINE FIELD access_class            ON dialogue_event_v2 TYPE string ASSERT $value INSIDE ["Public","Restricted","Confidential"];
DEFINE FIELD base                    ON dialogue_event_v2 TYPE object;
DEFINE FIELD payload                 ON dialogue_event_v2 TYPE object;
DEFINE FIELD enhancements            ON dialogue_event_v2 TYPE option<object>;
DEFINE FIELD metadata                ON dialogue_event_v2 TYPE option<object>;
DEFINE FIELD vectors                 ON dialogue_event_v2 TYPE option<object>;

-- 2. 主索引（tenant+session+sequence）
DEFINE INDEX idx_dialogue_event_v2_sequence
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, session_id, sequence_number
    UNIQUE;

-- 3. 时间轴索引（tenant+timestamp）
DEFINE INDEX idx_dialogue_event_v2_timestamp
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, timestamp_ms;

-- 4. 场景/类型索引
DEFINE INDEX idx_dialogue_event_v2_scenario
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, scenario, event_type;

-- 5. Supersession 链路
DEFINE INDEX idx_dialogue_event_v2_supersession
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, base.supersedes, base.superseded_by;

-- 6. Content Reference 反向检索
DEFINE INDEX idx_dialogue_event_v2_content_ref
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, payload.content_ref.message.message_id;

-- 7. Payload 工具结果检索
DEFINE INDEX idx_dialogue_event_v2_tool
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, payload.tool_invocation.tool_id, payload.tool_result.success;

-- 8. 时间窗口/增强维度索引
DEFINE INDEX idx_dialogue_event_v2_time_window
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, enhancements.time_window;

DEFINE INDEX idx_dialogue_event_v2_semantic_cluster
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, enhancements.semantic_cluster_id;

-- 9. 向量索引占位（需 SurrealDB 向量扩展支持）
DEFINE INDEX idx_dialogue_event_v2_content_vec
    ON TABLE dialogue_event_v2
    COLUMNS tenant_id, enhancements.content_embedding
    HNSW DIMENSION 1536 TYPE F32 M 16 EFC 200
    COMMENT "内容向量索引（需 SurrealDB 向量能力）";

-- 10. 灰度迁移视图：读取 legacy 表并映射至新结构（仅示例）
DEFINE TABLE dialogue_event_v2_legacy_mirror
    SCHEMALESS
    COMMENT "Legacy dialogue_event 映射视图"
    AS SELECT
        id,
        tenant_id,
        session_id,
        event_id,
        scenario,
        event_type,
        sequence_number,
        timestamp_ms,
        access_class,
        {
            event_id: event_id,
            supersedes: supersedes,
            superseded_by: superseded_by
        } AS base,
        {
            message_ref: message_ref,
            tool_invocation: tool_invocation,
            tool_result: tool_result,
            reasoning_trace: reasoning_trace,
            evidence_pointer: evidence_pointer,
            content_digest_sha256: content_digest_sha256,
            blob_ref: blob_ref
        } AS payload,
        {
            time_window: time_window,
            temporal_pattern_id: temporal_pattern_id,
            causal_links: causal_links,
            content_embedding: content_embedding,
            context_embedding: context_embedding,
            decision_embedding: decision_embedding,
            semantic_cluster_id: semantic_cluster_id,
            growth_stage: growth_stage,
            processing_latency_ms: processing_latency_ms,
            influence_score: influence_score,
            community_impact: community_impact
        } AS enhancements,
        metadata,
        NONE AS vectors
    FROM dialogue_event;

COMMIT;

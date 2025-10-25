-- Stage 1: DialogueEvent V2 schema草案
--
-- 仅用于讨论与评审，尚未应用到实际环境。

DEFINE TABLE dialogue_event_v2 SCHEMAFULL
    PERMISSIONS FULL
    COMMENT "DialogueEvent 三层结构主表 (Stage 1 草案)";

DEFINE FIELD id                 ON dialogue_event_v2 TYPE record<event>;
DEFINE FIELD base               ON dialogue_event_v2 TYPE object;
DEFINE FIELD base.tenant_id     ON dialogue_event_v2 TYPE string;
DEFINE FIELD base.event_id      ON dialogue_event_v2 TYPE string;
DEFINE FIELD base.session_id    ON dialogue_event_v2 TYPE string;
DEFINE FIELD base.config_version ON dialogue_event_v2 TYPE option<string>;
DEFINE FIELD base.stage_hint    ON dialogue_event_v2 TYPE option<string>;
DEFINE FIELD payload            ON dialogue_event_v2 TYPE object;
DEFINE FIELD payload.kind       ON dialogue_event_v2 TYPE string;
DEFINE FIELD enhancements       ON dialogue_event_v2 TYPE option<object>;
DEFINE FIELD enhancements.semantic.content_embedding ON dialogue_event_v2 TYPE option<array<float>>;
DEFINE FIELD enhancements.temporal.time_window       ON dialogue_event_v2 TYPE option<string>;
DEFINE FIELD enhancements.graph.causal_links         ON dialogue_event_v2 TYPE option<array<object>>;

-- TODO(Stage 2+):
--  * 各子对象字段的精细类型定义（枚举/校验规则）
--  * timeline/session/causal/vector 等索引策略
--  * 数据迁移脚本与双写回滚方案

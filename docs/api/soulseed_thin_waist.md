# Soulseed Thin-Waist API 文档

版本：v0.1（RIS 草案）
覆盖范围：支撑九种对话场景跑通的最小接口集合
引用模型：soulseed-agi-core-models, docs/schemas/*.json

## 1. 基础信息
- Base URL: https://{host}/api/v1
- 认证: HTTP 头 Authorization: Bearer [token]
- 租户: 统一通过路径段 /tenants/{tenant_id} 传递（亦可兼容 Header X-Tenant-Id）
- 内容类型: application/json; charset=utf-8

## 2. 核心枚举（节选）
| 枚举 | 取值 | 说明 |
| --- | --- | --- |
| ConversationScenario | human_to_human, human_group, human_to_ai, ai_to_ai, ai_self_talk, human_to_multi_ai, multi_human_to_multi_ai, ai_group, ai_to_system | 九种对话场景 |
| DialogueEventType | message, tool_call, tool_result, self_reflection, decision, lifecycle, system | 事件类型 |
| AccessClass | public, internal, restricted | 访问等级 |
| AwarenessEventType | 详见 docs/schemas/awareness/awareness_event.v1.schema.json | AC/IC/SyncPoint 事件 |

## 3. 响应结构示例
~~~json
{
  "success": true,
  "data": {"...": "payload"},
  "error": null,
  "trace_id": "uuid",
  "duration_ms": 12
}
~~~
失败示例:
~~~json
{
  "success": false,
  "error": {
    "code": "dialogue.invalid_input",
    "message": "invalid payload",
    "details": {"field": "scenario"}
  },
  "trace_id": "uuid"
}
~~~

## 4. Endpoint 总览
| 分类 | 方法与路径 | 说明 |
| --- | --- | --- |
| Core | POST /api/v1/triggers/dialogue | 以完整 `DialogueEvent` 触发觉知周期，服务端自动构建 `CycleRequest` + `SyncPointInput` |
| Core | GET /healthz | 运行状态检查 |
| Preview | GET /api/v1/ace/cycles/{cycle_id} | 返回 `CycleSchedule`、`SyncPointInput`、Outbox 摘要等快照 |
| Preview | GET /api/v1/ace/cycles/{cycle_id}/outbox | 拉取待发送的 Final / DeltaPatch / LateReceipt |
| Preview | GET /api/v1/ace/cycles/{cycle_id}/stream | SSE 订阅周期状态（pending/complete/timeout） |
| Preview | GET /api/v1/tenants/{tenant_id}/awareness/events | 按时间倒序列出近期 Awareness 事件 |
| Preview | POST /api/v1/ace/injections | 追加 HITL 注入，驱动后续 Clarify/协作流程 |
| TODO（规划中） | POST /api/v1/ace/cycles | 直接提交 `CycleRequest`，适合内部编排器 |

以下为已实现的核心接口说明，后续 TODO 接口保持设计占位，待对接 Soulbase 侧再扩充。

### 4.1 POST /api/v1/triggers/dialogue
以 DialogueEvent 触发 Clarify/Tool/SelfReason/Collab 等觉知周期，`ace_service` 会借助 `TriggerComposer` 生成 `CycleRequest`，并由 `AutoSyncDriver` 自动拼装 `SyncPointInput`，无需调用方再手动 `submit_callback`。

请求示例（与 `soulseed_agi_core_models::dialogue_event::DialogueEvent` 一致）：
```json
{
  "base": {
    "tenant_id": 42,
    "event_id": 1289461295035584512,
    "session_id": 99,
    "subject": {"AI": 7},
    "participants": [{"Human": 1, "role": "user"}],
    "head": {
      "envelope_id": "01955b1f-5f93-77b2-a73c-4b91f2a9b4f9",
      "trace_id": "trace-7d3f",
      "correlation_id": "corr-9b21",
      "config_snapshot_hash": "cfg-v1",
      "config_snapshot_version": 1
    },
    "snapshot": {"schema_v": 1, "created_at": "2025-03-01T00:00:00Z"},
    "timestamp_ms": 1740806400000,
    "scenario": "human_to_ai",
    "event_type": "message",
    "sequence_number": 5,
    "access_class": "internal",
    "provenance": {"source": "front-end", "method": "http"}
  },
  "payload": {
    "kind": "message_primary",
    "data": {
      "message": {"message_id": 512},
      "channel": "dialogue",
      "attributes": {"text": "你好，想学习 Soulseed 的开发流程。"}
    }
  },
  "metadata": {"ingest": "api"}
}
```

响应示例：
```json
{
  "cycle_id": 197556902645067776,
  "status": "completed",
  "manifest_digest": "auto:clarify:197556902645067776"
}
```

> **Notes**
> - 响应中的 `status` 为最新的 `CycleOutcome` 枚举字符串，`AutoSyncDriver` 会同步推进 `drive_until_idle`，因此常见返回值为 `completed`。
> - `manifest_digest` 来源于自动生成的 `SyncPointInput.context_manifest.manifest_digest`，用于后续 Explain 或查询上下文快照。
> - 若想直接提交 `CycleRequest`（例如内部服务链路），可使用下文 “数据结构参考” 的 JSON 模板命中 `/api/v1/ace/cycles`（TODO）。

### 4.2 GET /healthz
服务自检，返回
```json
{ "status": "ok" }
```

### 4.3 GET /api/v1/ace/cycles/{cycle_id}
返回一次触发后的快照，结构示例：
```json
{
  "schedule": { "cycle_id": 197556902645067776, "lane": "clarify", "status": "completed", "router_decision": {"context_digest": "ctx:..."}},
  "sync_point": { "kind": "clarify_answered", "events": [{ "base": {"event_type": "decision"}, "metadata": {"answer": "..."}}]},
  "outcomes": [
    {"cycle_id": 197556902645067776, "status": "completed", "manifest_digest": "auto:clarify:197556902645067776"}
  ],
  "outbox": [
    {"event_id": 281474976710672, "payload": {"event_type": "awareness_cycle_ended"}}
  ]
}
```

### 4.4 GET /api/v1/ace/cycles/{cycle_id}/outbox
仅返回 `outbox` 字段，方便前端轮询展示 Final/DeltaPatch/LateReceipt 等 Awareness 事件。

### 4.5 GET /api/v1/ace/cycles/{cycle_id}/stream
基于 SSE（Server-Sent Events）实时推送周期状态：

- `event: pending`：结果尚未生成（每 500ms 发送一次心跳）。
- `event: complete`：发送完整 `CycleSnapshot` JSON，并结束流。
- `event: timeout`：等待超时（默认 60 次轮询）后推送超时事件。

### 4.6 GET /api/v1/tenants/{tenant_id}/awareness/events
列出指定租户最近的 Awareness/Inference/SyncPoint 事件，默认返回最新 200 条，支持 `?limit=`（1~500）。

请求示例：`GET /api/v1/tenants/1/awareness/events?limit=100`

响应示例：
```json
{
  "success": true,
  "data": [
    {
      "tenant_id": 1,
      "event_id": 245040459783602176,
      "event_type": "awareness_cycle_started",
      "awareness_cycle_id": 245040459783602176,
      "occurred_at_ms": 1762486198000,
      "payload": {"lane": "Clarify"}
    }
  ],
  "error": null,
  "trace_id": null,
  "duration_ms": 2
}
```

> **Notes**
> - 数据来源于 `ace_awareness_event` 表，包含 AC/IC/SyncPoint 的完整 `AwarenessEvent` 结构。
> - `limit` 建议控制在 500 条以内，若未提供则默认返回 200 条。
> - 若租户不存在或服务端未开启持久化，会返回 4xx/5xx，并由 `error.code` 给出诊断。

### 4.7 POST /api/v1/ace/injections
向指定周期追加 HITL 注入，用于覆盖 Clarify 或人工补充指令。

请求示例：
```json
{
  "cycle_id": 197556902645067776,
  "priority": "p1_high",
  "author_role": "facilitator",
  "payload": {
    "type": "clarify_override",
    "text": "请用 Markdown 列表形式回答之前的问题，并补充后续步骤。"
  }
}
```

响应返回最新的 `CycleSnapshot`，即与 `GET /api/v1/ace/cycles/{id}` 相同的结构。

## 5. 数据结构参考

### 5.1 CycleRequest（内部结构）
`TriggerComposer::cycle_request_from_message` 会生成与下例类似的结构，供 `AceEngine::schedule_cycle` 使用：
```json
{
  "router_input": {
    "anchor": {
      "tenant_id": 42,
      "envelope_id": "01955b1f-5f93-77b2-a73c-4b91f2a9b4f9",
      "config_snapshot_hash": "cfg-v1",
      "config_snapshot_version": 1,
      "session_id": 99,
      "sequence_number": 5,
      "access_class": "internal",
      "provenance": {"source": "front-end", "method": "http"},
      "schema_v": 1,
      "scenario": "human_to_ai"
    },
    "context": {
      "anchor": { "...": "ContextAnchor 与上方 anchor 对齐" },
      "schema_v": 1,
      "version": 1,
      "segments": [
        {
          "partition": "p4_dialogue",
          "items": [
            {
              "ci_id": "msg:1289461295035584512",
              "partition": "p4_dialogue",
              "summary_level": null,
              "tokens": 0,
              "score_scaled": 0,
              "ts_ms": 1740806400000,
              "typ": "message",
              "why_included": "trigger_message"
            }
          ]
        }
      ],
      "explain": {
        "reasons": ["message_trigger"],
        "degradation_reason": null,
        "indices_used": [],
        "query_hash": null
      },
      "budget": {"target_tokens": 2048, "projected_tokens": 0},
      "prompt": {"dialogue": ["event:1289461295035584512"]}
    },
    "context_digest": "ctx:1289461295035584512",
    "scenario": "human_to_ai",
    "scene_label": "auto.generated",
    "user_prompt": "message:1289461295035584512",
    "budget": {"max_tokens": 4096, "max_walltime_ms": 60000, "max_external_cost": 25.0},
    "routing_seed": 1289461295035584512
  },
  "candidates": [],
  "budget": {
    "tokens_allowed": 6000,
    "tokens_spent": 0,
    "walltime_ms_allowed": 120000,
    "walltime_ms_used": 0,
    "external_cost_allowed": 50.0,
    "external_cost_spent": 0.0
  },
  "parent_cycle_id": null,
  "collab_scope_id": null
}
```

### 5.2 SyncPointInput（AutoSyncDriver 输出）
`AutoSyncDriver` 会根据 `CycleSchedule` 的 lane/plan 自动生成一个 `SyncPointInput`，其中 `events` 的最后一个元素即最终输出：
```json
{
  "cycle_id": 197556902645067776,
  "kind": "clarify_answered",
  "anchor": { "...": "与 CycleSchedule.anchor 对齐" },
  "events": [
    {
      "base": {
        "tenant_id": 42,
        "event_id": 197556902712969216,
        "session_id": 99,
        "subject": {"AI": 0},
        "participants": [{"Human": 1, "role": "user"}],
        "head": {
          "envelope_id": "01955b1f-5f93-77b2-a73c-4b91f2a9b4f9",
          "trace_id": "4eab7d7d-6f33-40c4-bc82-0c107e41a3b7",
          "correlation_id": "51ab2d9a-5f6f-44a0-8f4d-2c9f0a0f87a9",
          "config_snapshot_hash": "cfg-v1",
          "config_snapshot_version": 1
        },
        "snapshot": {"schema_v": 1, "created_at": "2025-03-01T00:00:01Z"},
        "timestamp_ms": 1740806401000,
        "scenario": "human_to_ai",
        "event_type": "decision",
        "stage_hint": "clarify",
        "access_class": "internal",
        "sequence_number": 6,
        "ac_id": 197556902645067776,
        "ic_sequence": 1,
        "parent_ac_id": null
      },
      "payload": {
        "kind": "clarification_answered",
        "data": {
          "clarification_id": "clarify-197556902645067776",
          "attributes": {
            "questions": [
              "请确认目标学习主题。",
              "是否需要示例代码？"
            ],
            "limits": {"wait_ms": null, "total_wait_ms": null}
          }
        }
      },
      "metadata": {
        "auto_generated": true,
        "cycle_lane": "clarify",
        "cycle_id": 197556902645067776,
        "router_digest": "router:digest",
        "context_digest": "ctx:1289461295035584512",
        "decision_issued_at_ms": 1740806400500
      }
    }
  ],
  "budget": {
    "tokens_allowed": 6000,
    "tokens_spent": 0,
    "walltime_ms_allowed": 120000,
    "walltime_ms_used": 0,
    "external_cost_allowed": 50.0,
    "external_cost_spent": 0.0
  },
  "timeframe": ["2025-03-01T00:00:00Z", "2025-03-01T00:00:01Z"],
  "pending_injections": [],
  "context_manifest": {
    "manifest_digest": "auto:clarify:197556902645067776",
    "auto_generated": true,
    "prepared_at_ms": 1740806401000,
    "router_digest": "router:digest",
    "context_digest": "ctx:1289461295035584512",
    "cycle_lane": "clarify",
    "cycle_id": 197556902645067776
  },
  "parent_cycle_id": null,
  "collab_scope_id": null
}
```

> `events` 数组最后一项会被 `AceService::produce_final_event` 复用为最终对话输出；如需扩展工具或协作场景，可在 `AutoSyncDriver` 中注入额外事件（例如 `tool_result_recorded`、`collab_resolved`）。

## 6. 九种对话场景推荐调用顺序
| 场景 | 主要接口 | 说明 |
| --- | --- | --- |
| 人类↔人类私聊 | POST /api/v1/triggers/dialogue | scenario=human_to_human，多为记录型，不进入 ACE |
| 人类群聊 | 同上 + TODO: SSE | 关注多 subject/participants，计划接入 SSE |
| 人类↔AI 私域 | POST /api/v1/triggers/dialogue（Clarify） | AutoSyncDriver 自动产出 Clarify 回答 |
| AI↔AI 私聊 | POST /api/v1/triggers/dialogue（SelfReason） | 自省/互评场景，待补充 Explain |
| 人类↔工具链 | POST /api/v1/triggers/dialogue（Tool 路径） | 需扩展 AutoSyncDriver 生成 tool_result 事件 |
| 协作/多 Agent | POST /api/v1/triggers/dialogue（Collab 路径） | SyncPointInput 支持 collab_scope_id |
| AI↔系统 | 同上，payload 中含 tool 调用信息 | 可结合 Redis outbox 推送到 Soulbase |

## 7. 常见错误码
| Code | HTTP | 说明 |
| --- | --- | --- |
| dialogue.invalid_input | 400 | 请求体校验失败 |
| dialogue.conflict | 409 | event_id 幂等冲突 |
| graph.missing_index | 400 | 查询未命中索引（禁止线扫） |
| ace.cycle_busy | 409 | Lane 正在运行 |
| ace.budget_exceeded | 429 | 周期预算耗尽 |
| context.pointer_invalid | 400 | ContextItem 证据指针失效 |
| auth.unauthorized | 401 | Token 无效或权限不足 |

## 8. 变更记录
| 版本 | 日期 | 内容 |
| --- | --- | --- |
| v0.1 | 2025-03-01 | 首版，覆盖九场景最小接口集 |

后续若扩展 LLM/Tools Thin-Waist 直接调用、Graph Explain 下载等能力，可在本文件新增章节。

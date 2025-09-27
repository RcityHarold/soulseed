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
| Dialogue | POST /tenants/{tenant_id}/dialogue-events | 写入对话事件 |
|  | GET /tenants/{tenant_id}/dialogue-events/{event_id} | 查询单条事件 |
| Graph | GET /tenants/{tenant_id}/graph/timeline | 时间线查询 |
|  | GET /tenants/{tenant_id}/graph/causal | 因果链路查询 |
|  | GET /tenants/{tenant_id}/graph/recall | 历史召回（向量/BM25/Hybrid） |
| Context | GET /tenants/{tenant_id}/context/bundle | 获取最新 ContextBundle |
|  | POST /tenants/{tenant_id}/context/manifest/compact | 触发上下文压缩 |
| ACE Cycle | POST /tenants/{tenant_id}/ace/cycles | 启动觉知周期 |
|  | GET /tenants/{tenant_id}/ace/cycles/{cycle_id} | 查询周期状态和 SyncPointReport |
|  | GET /tenants/{tenant_id}/ace/cycles/{cycle_id}/outbox | 查询待发 Final/DeltaPatch |
| HITL | POST /tenants/{tenant_id}/ace/injections | 提交人类注入 |
| Awareness | GET /tenants/{tenant_id}/awareness/events | 查询 AwarenessEvent |
| Explain | GET /tenants/{tenant_id}/explain/indices | 查看 indices_used/query_hash/degrade |
| Streaming | GET /tenants/{tenant_id}/live/dialogues/{session_id} | SSE 订阅 Dialogue 与 Awareness |

以下为关键接口详解。

### 4.1 POST /tenants/{tenant_id}/dialogue-events
用于写入 DialogueEvent。九种对话场景统一使用该接口。
~~~json
{
  "event_id": "ev-20250301-0001",
  "session_id": "sess-20250301-001",
  "scenario": "human_to_ai",
  "event_type": "message",
  "timestamp_ms": 1740806400000,
  "subject": {"kind": "human", "human_id": "h-0001"},
  "participants": [{"kind": "ai", "ai_id": "ai-mentor", "role": "assistant"}],
  "head": {
    "envelope_id": "env-0001",
    "trace_id": "trace-123",
    "correlation_id": "corr-123",
    "config_snapshot_hash": "cfg-v1",
    "config_snapshot_version": 1
  },
  "snapshot": {
    "schema_v": 1,
    "created_at": "2025-03-01T00:00:00Z"
  },
  "sequence_number": 1,
  "access_class": "internal",
  "metadata": {"text": "你好，我需要学习 Soulseed 的开发流程。"}
}
~~~
成功响应返回补全后的 DialogueEvent（含 reasoning_trace 等）。同一 tenant_id + event_id 支持幂等覆盖。

### 4.2 GET /tenants/{tenant_id}/graph/timeline
- 查询参数: session_id、scenario、since_ms、until_ms、limit
- 响应: items 列表和 next_cursor（命中索引 idx_dialogue_event_timeline）

### 4.3 GET /tenants/{tenant_id}/graph/causal
- 查询参数: root_event_id、direction=forward|backward|both、depth、scenario
- 响应: nodes 与 edges，用于绘制因果链

### 4.4 GET /tenants/{tenant_id}/graph/recall
- 查询参数: mode=vector|bm25|hybrid、query_text、embedding、k、filters(json)
- 响应: 召回数组，包含 score、source_event_id、indices_used、query_hash、degradation_reason

### 4.5 POST /tenants/{tenant_id}/ace/cycles
- 用于触发 Clarify/Tool/SelfReason/Collab 周期
~~~json
{
  "lane": "clarify",
  "anchor": {"tenant_id": "tenant-a", "envelope_id": "env-clarify-01", "schema_v": 1},
  "tool_plan": null,
  "llm_plan": {
    "model_id": "openai:gpt-4.1",
    "budget": {"tokens": 4096, "walltime_ms": 15000}
  },
  "budget": {
    "tokens_allowed": 6000,
    "tokens_spent": 0,
    "walltime_ms_allowed": 30000,
    "walltime_ms_used": 0
  }
}
~~~
响应: {"cycle_id": "cyc-clarify-001", "accepted": true, "reason": null}

### 4.6 GET /tenants/{tenant_id}/ace/cycles/{cycle_id}
返回周期信息、最新 SyncPointReport、AwarenessEvent 列表、当前 HITL 队列。

### 4.7 GET /tenants/{tenant_id}/ace/cycles/{cycle_id}/outbox
返回待发送的 Final、DeltaPatch、LateReceipt，便于前端展示最终答案或迟到回执。

### 4.8 GET /tenants/{tenant_id}/context/bundle
- 查询参数: anchor.envelope_id、manifest_digest(可选)
- 响应: bundle.segments, bundle.explain.indices_used, bundle.explain.degradation_reason 等

### 4.9 POST /tenants/{tenant_id}/context/manifest/compact
手动触发上下文压缩，例如 {"anchor": {...}, "target_level": "L2"}。

### 4.10 POST /tenants/{tenant_id}/ace/injections
提交人类注入请求。
~~~json
{
  "cycle_id": "cyc-clarify-001",
  "priority": "p1_high",
  "author_role": "facilitator",
  "payload": {
    "type": "clarify_override",
    "text": "请工具输出 Markdown，并限制在 300 token 内"
  }
}
~~~
响应: {"injection_id": "inj-20250301-0003", "status": "queued"}

### 4.11 GET /tenants/{tenant_id}/awareness/events
支持按照 cycle_id、event_type、since_ms、until_ms、collab_scope_id、barrier_id、limit 过滤。

### 4.12 GET /tenants/{tenant_id}/explain/indices
用于调试 Explain 链路。
~~~json
{
  "graph": {
    "indices_used": ["idx_dialogue_event_timeline"],
    "query_hash": "timeline:hash",
    "degradation_reason": null
  },
  "context": {
    "indices_used": ["context_manifest_digest"],
    "query_hash": "context:compact:v3",
    "degradation_reason": "graph_sparse_only"
  },
  "dfr": {
    "router_digest": "blake3:2f9d...",
    "degradation_reason": "budget_tokens"
  },
  "ace": {
    "sync_point": "clarify_answered",
    "degradation_reason": null
  }
}
~~~

### 4.13 GET /tenants/{tenant_id}/live/dialogues/{session_id}
Server-Sent Events 订阅对话与觉知事件。事件类型:
- event: dialogue_event → data: {...DialogueEvent...}
- event: awareness_event → data: {...AwarenessEvent...}
- event: ping → data: {"ts": 1740806400000}

## 5. 九种对话场景推荐调用顺序
| 场景 | 主要接口 | 说明 |
| --- | --- | --- |
| 人类↔人类私聊 | POST dialogue-events → GET graph/timeline | scenario=human_to_human，通常无 ACE 周期 |
| 人类群聊 | 同上 + GET live/dialogues/{session} | 关注多 subject/participants |
| 人类↔AI 私域 | POST dialogue-events → POST ace/cycles (clarify) → SSE | 展示 Clarify 闭环 |
| AI↔AI 私聊 | 两个 AI 互发事件 → GET graph/causal | scenario=ai_to_ai，关注 decision 事件 |
| AI 自省 | POST ace/cycles (self_reason) → GET awareness/events | 观察 ic_started/ic_ended 序列 |
| 人类↔多 AI | POST dialogue-events (多 AI 参与) → POST ace/cycles (collab) | awareness.events 显示协同 barrier |
| 多人类↔多 AI | 同上，scenario=multi_human_to_multi_ai | 前端需处理多租户/角色视图 |
| AI 群组 | scenario=ai_group，结合 SSE 展示 AI 协作 |
| AI↔系统 | scenario=ai_to_system，包含大量 tool_call/tool_result | 可配合 explain/indices 观察降级链 |

## 6. 常见错误码
| Code | HTTP | 说明 |
| --- | --- | --- |
| dialogue.invalid_input | 400 | 请求体校验失败 |
| dialogue.conflict | 409 | event_id 幂等冲突 |
| graph.missing_index | 400 | 查询未命中索引（禁止线扫） |
| ace.cycle_busy | 409 | Lane 正在运行 |
| ace.budget_exceeded | 429 | 周期预算耗尽 |
| context.pointer_invalid | 400 | ContextItem 证据指针失效 |
| auth.unauthorized | 401 | Token 无效或权限不足 |

## 7. 变更记录
| 版本 | 日期 | 内容 |
| --- | --- | --- |
| v0.1 | 2025-03-01 | 首版，覆盖九场景最小接口集 |

后续若扩展 LLM/Tools Thin-Waist 直接调用、Graph Explain 下载等能力，可在本文件新增章节。

# Awareness Event Schema v1

本文件描述觉知层（ACE/DFR/CA/HITL）事件的 Append-Only 结构以及推荐索引策略。对应 JSON Schema 位于 `docs/schemas/awareness/awareness_event.v1.schema.json`，版本号 `v1`。

## 六锚与公共字段

所有事件通过 `AwarenessAnchor` 携带六锚（附带访问控制字段）：

- `tenant_id`：租户标识，索引主分区键。
- `envelope_id`：事件 Envelope ID（v7 UUID）。
- `config_snapshot_hash`：运行配置快照哈希。
- `config_snapshot_version`：运行配置版本号。
- `session_id`（可空）：关联会话标识。
- `sequence_number`（可空）：Envelope 内单调序号。

Anchor 还包含：

- `access_class`：访问级别；当为 `restricted` 时，必须同时携带 `provenance`。
- `provenance`（可空）：受限事件的来源说明。
- `schema_v`：Anchor 自身的 schema 版本。

通用字段：

| 字段 | 说明 |
| ---- | ---- |
| `event_id` | Append-Only 主键，对齐 DialogueEvent 中的 `EventId`。 |
| `event_type` | 觉知层事件枚举，覆盖 AC/IC 时钟、计划路由、Clarify/Tool/Collab、HITL、SyncPoint、ENVIRONMENT。 |
| `occurred_at_ms` | 事件发生时间（UTC 毫秒），单调递增以支撑回放。 |
| `awareness_cycle_id` | 当前 AC 标识（`CycleId`）。 |
| `parent_cycle_id` | 父 AC（协同/分形）引用，可空。 |
| `collab_scope_id` | 协同 Scope 唯一标识，可空。 |
| `barrier_id` | 工具/协同 Barrier 标识，可空。 |
| `env_mode` | 环境模式（如 `generic`、`turbo`、`sandbox`），可空。 |
| `inference_cycle_sequence` | 当前 IC 序号（从 1 开始）。 |
| `degradation_reason` | 降级原因枚举，确保 Explain 外泄。 |
| `payload` | 事件载荷，结构随 `event_type` 变化，缺省 `{}`。 |

## 索引建议

建议数据库创建以下索引（JSON Schema 的 `required_indices` 字段已同步列出）：

1. `ix_awareness_cycle_time`：（`tenant_id`, `awareness_cycle_id`, `occurred_at_ms`）
2. `ix_awareness_parent_cycle`：（`tenant_id`, `parent_cycle_id`, `occurred_at_ms`），过滤 `parent_cycle_id IS NOT NULL`
3. `ix_awareness_collab_scope`：（`tenant_id`, `collab_scope_id`, `occurred_at_ms`），过滤 `collab_scope_id IS NOT NULL`
4. `ix_awareness_barrier`：（`tenant_id`, `barrier_id`, `occurred_at_ms`），过滤 `barrier_id IS NOT NULL`
5. `ix_awareness_env_mode`：（`tenant_id`, `env_mode`, `occurred_at_ms`），过滤 `env_mode IS NOT NULL`

这些索引确保：

- O(1) 追溯父子 AC 与协同范围；
- 快速聚合 Barrier / SyncPoint；
- 根据环境模式做差异化回放与审计。

## 事件类型速览

| `event_type` | 说明 |
| ------------ | ---- |
| `awareness_cycle_started` / `awareness_cycle_ended` | AC 生命周期事件，标记新的 AC 启动/封印。 |
| `inference_cycle_started` / `inference_cycle_completed` | IC 时钟事件，记录 routing_seed、ic 序号、walltime 统计。 |
| `assessment_produced` | DFR 评估摘要，外泄得分与阻断原因。 |
| `decision_routed` / `tool_path_decided` | 主分叉选择结果，附 `DecisionPath` 三指纹（Plan/Budget/Explain）。 |
| `route_reconsidered` / `route_switched` | 路由重评或切换，附 degrade/explain。 |
| `tool_called` / `tool_responded` / `tool_failed` | 工具执行链，包含 `ToolPlan` 节点、evidence 指针、迟到标记。 |
| `tool_barrier_reached` / `tool_barrier_released` / `tool_barrier_timeout` | 工具 Barrier 生命周期，驱动 Saga/补偿。 |
| `collab_requested` / `collab_resolved` | 协同发起与回合归并，追踪 `collab_scope_id`、Barrier 状态。 |
| `clarification_issued` / `clarification_answered` | Clarify 闸门记录，含问题内容与回答摘要。 |
| `human_injection_received` / `human_injection_applied` / `human_injection_deferred` / `human_injection_ignored` | HITL 注入生命周期，关联队列优先级与 `DeltaPatch` 状态。 |
| `delta_patch_generated` / `context_built` / `delta_merged` | CA `merge_delta` 输出、ContextManifest 生成与合流。 |
| `sync_point_merged` / `sync_point_reported` | SyncPoint 吸收结果，写入 `SyncPointReport`（inbox_stats、budget_snapshot）。 |
| `finalized` / `rejected` | AC 收敛结果（Final 唯一语义）或拒绝，记录最终 Explain/指纹。 |
| `late_receipt_observed` | Final 之后迟到回执审计，标注原因与补救动作。 |
| `environment_snapshot_recorded` | EnvCtx 稳定快照，包含 `ENVIRONMENT_SNAPSHOT_EVENT` digest。 |

> `payload` 子结构与类型定义可在 `DecisionPath`、`DeltaPatch`、`SyncPointReport` 等模型中查阅；后续版本将补充更多 JSON 样例。

## 版本与演进策略

- `v1` 为觉知层最小闭环的基线。
- 新增字段遵循 Append-Only 原则，保持向后兼容；弃用字段使用 `deprecated` 注解并保留一版兼容期。
- 重大变更（例如 payload 结构重构）需要新增 `schema_v` 与 JSON Schema 版本，并在运行时依据 `schema_v` 或 `event_type` 进行兼容处理。

> 后续工作：将 `payload` 子结构拆分 `$ref`、补充事件样例，并在 CI 中加入 schema 校验，用于保障与 `AwarenessEventType` 枚举的同步。

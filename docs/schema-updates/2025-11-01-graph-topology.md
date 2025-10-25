# 2025-11-01 Graph Topology Schema 草案

## 目标
- 引入七类节点（Event、Message、AwarenessCycle、Session、Actor、Context、Artifact）。
- 支撑 30+ 图谱边类型，并统一携带 `strength` / `confidence` / `temporal_decay` 属性。
- 为查询端提供多维 Explain 能力，支持按租户、节点类型、边类型的高效过滤。

## 数据表

### `graph_node`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | record<string> | 节点唯一标识（type:id 形式） |
| `tenant_id` | record<string> | 所属租户 |
| `kind` | string | 节点类型（event/message/...） |
| `label` | option<string> | 展示名称 |
| `summary` | option<string> | 节点摘要 |
| `weight` | option<float> | 节点权重 0~1 |
| `metadata` | option<object> | 结构化扩展字段 |
| `created_at` | datetime | 创建时间 |
| `updated_at` | datetime | 更新时间 |

### `graph_edge`
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | record<string> | 边 ID（type:from:to:kind） |
| `tenant_id` | record<string> | 所属租户 |
| `from_ref` | record<string> | 起点（同 `graph_node.id`） |
| `to_ref` | record<string> | 终点 |
| `kind` | string | 边类型（causal/supports/...） |
| `strength` | option<float> | 0~1 |
| `confidence` | option<float> | 0~1 |
| `temporal_decay` | option<float> | >=0 |
| `since_ms` | option<int> | 生效开始 |
| `until_ms` | option<int> | 生效结束 |
| `explain` | option<string> | Explain 语句 |
| `properties` | option<object> | 业务扩展字段 |
| `created_at` | datetime | 创建时间 |
| `updated_at` | datetime | 更新时间 |

## 索引规划
- `graph_node`：
  - `idx_graph_node_tenant_kind`：(`tenant_id`, `kind`)
  - `idx_graph_node_updated_at`：`updated_at`（供 CDC / TTL）
- `graph_edge`：
  - `idx_graph_edge_from`：(`tenant_id`, `from_ref`, `kind`)
  - `idx_graph_edge_to`：(`tenant_id`, `to_ref`, `kind`)
  - `idx_graph_edge_kind_strength`：(`tenant_id`, `kind`, `strength`)
  - `idx_graph_edge_time_window`：(`tenant_id`, `kind`, `since_ms`, `until_ms`)

## 变更步骤
1. 创建两张表并回填现有数据（通过旧 schema 导出的 CSV）。
2. 构建 out-degree / in-degree 物化视图，供 Explain 与密度监测使用。
3. 更新 `soulseed-agi-graph` 查询计划，优先选择 `idx_graph_edge_from` / `idx_graph_edge_to`。
4. 建立巡检任务：检测 `strength` / `confidence` 是否越界、`until_ms` < `since_ms` 的脏数据。

## 后续
- 在 `docs/playbooks/dialogue_event_stage5.md` 添加图谱双写演练。
- 结合 Analytics Service 输出图谱密度指标。

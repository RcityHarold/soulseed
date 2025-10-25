# Stage 5 · 服务层双写与演练指南

> 目标：在正式切换到 `dialogue_event_v2` 前后，保障写链路与回滚流程可控。

## 双写策略
1. **Feature Gate**：统一使用 `event-v2` feature 触发 V2 构造与验证。
2. **工具链路**：默认输出即为 V2 结构；如需保留 legacy，可在服务内部使用转换器 `convert_legacy_dialogue_event` 进行回放。
3. **ACE/LLM**：统一调用 `validate_dialogue_event` 校验并写入 V2 结构。如需回滚，可使用转换器生成 legacy 事件。

## 部署步骤
1. **阶段 A（监测）**：仅开启校验日志，统计失败比率。
2. **阶段 B（隐式双写）**：Event Writer 新增队列将 V2 事件写入影子表，验证查询对齐。
3. **阶段 C（切换）**：将影子表升级为主表写入来源，同时保留 legacy 写入一段时间。
4. **阶段 D（回滚）**：提供脚本从 `dialogue_event_v2` 回放至 legacy 表，确保可逆。

## 验证清单
- `cargo check --features event-v2`（core-models/tools/ace/llm/context/graph）
- 双写环境：比较 `SELECT COUNT(*)` 及哈希校验（样本）。
- 演练：执行回滚脚本，确认服务可读取 legacy 数据继续运行。

## 附录
- SQL 草案：`migrations/surreal/002_dialogue_event_v2.sql`
- 回归手册：`docs/regression/dialogue_event_migration.md`

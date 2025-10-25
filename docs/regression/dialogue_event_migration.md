# DialogueEvent 迁移回归清单（Stage 0）

> 针对 `DialogueEvent` 三层结构改造的增量测试待办，用于后续阶段逐步覆盖。

## 核心数据层
- `soulseed-agi-core-models`: 合同测试 `tests/contract.rs`；示例 `examples/min.rs`。
- `soulseed-agi-core-models`: Schema 校验（SurrealDB DDL 草案、向量索引）。

## 事件生产链路 (Stage 2)
- Tools Engine：统一调用 `validate_dialogue_event`，在写入前确保事件合法。
- ACE Engine/Aggregator：写出前执行同样校验，保证投产事件满足约束。
- LLM Final Event：最终写库前校验并记录元数据。
- 回归：`cargo check`（默认已启用 event-v2）以及灰度验证。

## 事件消费链路 (Stage 3)
- Graph MockExecutor：入库前执行 `validate_dialogue_event` 校验，避免不合规事件写入。
- ACE 引擎及聚合器：读取端复用核心校验，确保回放/总结时结构一致。
- 回归：`cargo check`（graph/context/ace），对关键查询进行抽样比对。

## Schema 切换 (Stage 4)
- SQL 迁移脚本：`migrations/surreal/002_dialogue_event_v2.sql`。
- 步骤：先建新表与索引 → 使用 VIEW 镜像 legacy 数据 → 逐步切换写入。
- 回归：对比 `dialogue_event` 与 `dialogue_event_v2` 查询结果，验证回滚脚本。

## 服务层演练 (Stage 5)
- 双写方案：详见 `docs/playbooks/dialogue_event_stage5.md`，基于默认 V2 结构开展回滚/双写演练。
- Event Writer 服务：启用租户级灰度开关，验证写入影子表与回滚脚本。
- 回归：`cargo check`（全链路）+ 双写环境哈希对比/回放演练。

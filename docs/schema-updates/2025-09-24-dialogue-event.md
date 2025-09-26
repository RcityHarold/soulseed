# 2025-09-24 DialogueEvent 契约更新摘要

## 变更概览
- 为 `DialogueEvent` 新增字段：
  - `scenario: ConversationScenario` —— 对应九种对话场景的强类型标识。
  - `event_type: DialogueEventType` —— 区分消息、工具调用、工具结果、自省、决策、生命周期、系统事件。
  - 结构化载体：
    - `message_ref`（`MessagePointer`）引用源消息。
    - `tool_invocation`、`tool_result` 描述工具调用与结果。
    - `self_reflection` 记录自省内容与信心度。
- `Session.scenario` 改为 `Option<ConversationScenario>`，不再使用裸字符串。
- 针对不同 `event_type` 的校验逻辑：
  - `Message` 必须提供 `message_ref`。
  - `ToolCall` 必须提供 `tool_invocation`。
  - `ToolResult` 必须提供 `tool_result`。
  - `SelfReflection` 必须提供 `self_reflection`。

## 影响范围
- 所有下游 crate（graph/context/tools/authz）需引用 `soulseed-agi-core-models` 中的统一类型，禁止本地复制结构。
- 前端 / API 在写入事件时需要按 `event_type` 补齐结构化 payload。

## 升级指导
1. 更新依赖：将相关 crate 升级到最新 `soulseed-agi-core-models`，移除本地重复定义。
2. 写入事件前明确当前对话场景并填充 `scenario` 与 `event_type`：
   - `event.scenario = ConversationScenario::HumanToAi;`
   - `event.event_type = DialogueEventType::ToolResult;`
   - `event.tool_result = Some(result_payload);`
3. 所有结构化字段（如 `tool_invocation`）未使用时保持 `None`，避免写入空 JSON。
4. 接入层如仍使用旧版字符串场景，需要在序列化前映射到 `ConversationScenario`。

## 后续计划
- Graph 层将基于 `scenario` 提供过滤与 LIVE 订阅能力。
- Context 压缩策略会结合不同场景的分区权重做调优。
- Tools Orchestrator 将输出以 `DialogueEvent` 表达的工具执行/协同事件，统一写回图谱。

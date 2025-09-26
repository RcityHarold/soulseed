# Soulseed AGI 后端总览

本仓库承载 Soulseed AGI 编排侧的最小可运行实现，目标是在 **DialogueEvent** 作为唯一事实账本的前提下，串联基石层与觉知层：

- 覆盖九种对话场景（Human↔Human、Human↔AI、AI↔AI、协作、自省等），在 DC→AC→IC 的节律中完成事件编排。
- 以 Append-Only、六锚统一、同步点一次吸收和降级原因外泄为工程宪法，保证全链路可回放、可解释。
- 输出计划/事件/解释，通过 Soulbase 薄腰接口驱动 LLM、工具、Graph、AuthZ、EnvCtx 等服务。

> ⚠️ 当前仓库已实现全部后端逻辑及合同测试，但仍使用本地/Mock 适配器。要在真实环境中运行，需要落地 Soulbase Gateway 与 SurrealDB 索引，详见下文。

## 目录结构

~~~
01-SoulseedAGI开发架构文档/   项目蓝图与需求原始文档
soulseed-agi-core-models/      核心数据契约（DialogueEvent、AwarenessEvent 等）
soulseed-agi-graph/            对话图谱查询与 Explain 回放
soulseed-agi-context/          Context Assembly（Build/Merge/Compact、Manifest、Explain）
soulseed-agi-envctx/           环境上下文装配器与 Thin-Waist 适配层
soulseed-agi-ace/              觉知周期引擎（调度、SyncPoint、HITL、Outbox）
soulseed-agi-dfr/              分叉决策器（Router/Planner/Explain）
soulseed-agi-tools/            工具编排（Router/Planner/Orchestrator）
soulseed-agi-llm/              LLM 编排与 Thin-Waist 客户端
soulseed-agi-authz/            权限与配额策略引擎
docs/                          合同说明、schema 与补充文档
src/ca.rs                      ACE→CA 契约副本（便于查阅）
~~~

## 核心模块概览

| 模块 | 主要职责 | 关键特性 |
| --- | --- | --- |
| soulseed-agi-core-models | 定义 DialogueEvent、AwarenessEvent、DecisionPath 等契约 | 六锚校验、九场景枚举、工具/澄清/HITL 事件支持 |
| soulseed-agi-graph | 提供 timeline/causal/recall/awareness 查询 | 强制索引命中，Explain 外泄 indices_used、query_hash、degradation_reason |
| soulseed-agi-context | 构建对话上下文、执行五分区六步法压缩 | Manifest digest、Redaction、Env Snapshot、Graph Explain 透传 |
| soulseed-agi-envctx | 按 RIS 规范拼装 EnvironmentContext | Thin-Waist Provider/Reporter 接口、退化可观测、digest 计算 |
| soulseed-agi-ace | 觉知周期编排：调度、预算、同步点、Outbox | Clarify 单通道、HITL 注入、迟到回执、DeltaPatch 吸收 |
| soulseed-agi-dfr | 分叉路由：自省/澄清/工具/协同计划 | 三指纹（router/router_config/routing_seed）、预算评估、降级树 |
| soulseed-agi-tools | 工具路由 & 执行 | DAG/Barrier、Explain 聚合、Thin-Waist tools.precharge/execute |
| soulseed-agi-llm | LLM 计划执行与回传 | 模型选路、降级链拼接、Explain indices/query hash 合并 |
| soulseed-agi-authz | 鉴权与配额 | 资源 URN、Clarify 并发/重试上限、协同规模预算 |

## 业务数据流（DC → AC → IC）

1. **核心账本**：所有对话、工具、协同及觉知事件以 DialogueEvent 和 AwarenessEvent Append-Only 写入，确保可重放。
2. **Graph 层**：通过 SurrealDB 索引提供时间线、因果、召回、场景筛选，并生成 Explain 元信息。
3. **Context Assembly**：结合环境上下文（EnvCtx）与 Graph Explain 构建 ContextBundle，输出 Manifest / Redaction / ENVIRONMENT_SNAPSHOT_EVENT。
4. **分叉决策（DFR）**：在场景、预算、策略约束下挑选自省/澄清/工具/协同计划，生成决策指纹。
5. **ACE**：调度觉知周期、吸收 SyncPoint、处理 HITL 注入、输出 Final 事件，同时驱动 Tools/LLM。
6. **Explain 贯穿**：indices_used、query_hash、degradation_reason 在 Graph → Context → DFR → Tools/LLM → ACE 全链路透传，便于追踪降级与命中索引。

## 外部依赖与落地注意

- **Soulbase Gateway**：生产环境需要接入 Soulbase 提供的 Thin-Waist /llm、/tools、/graph.recall、/observe、/repo.append 等接口。本仓库默认使用 Mock 实现以通过合同测试。
- **SurrealDB 索引**：Graph 层严格要求查询命中 timeline/causal/recall/awareness 等索引，禁止线扫。在真实部署中请提前创建对应索引。
- **环境与权限**：EnvCtx、AuthZ、Quota 等模块需在 Soulbase 侧配置资源、策略与配额，否则会触发降级。

> 在没有 Soulbase / SurrealDB 的情况下，可以运行所有 cargo test 做逻辑验证，但无法完成真实对话闭环。

## 开发环境

- Rust nightly（支持 Edition 2024），推荐使用 rustup override set nightly 或指定日期的 nightly 版。
- 基本工具：cargo, rustfmt, clippy（可选）。
- 可选：SurrealDB、本地 Thin-Waist mock 服务，仅在手动联调时使用。

## 构建与测试

仓库目前未创建统一 workspace，需要在各 crate 目录执行测试：

~~~bash
# 进入具体模块后运行
cd soulseed-agi-core-models && cargo test
cd ../soulseed-agi-graph && cargo test
cd ../soulseed-agi-context && cargo test
cd ../soulseed-agi-envctx && cargo test
cd ../soulseed-agi-ace && cargo test
cd ../soulseed-agi-dfr && cargo test
cd ../soulseed-agi-tools && cargo test
cd ../soulseed-agi-llm && cargo test
cd ../soulseed-agi-authz && cargo test
~~~

所有测试均聚焦契约验证、Explain 外泄、降级路径、预算与权限策略，确保最小可运行版本稳定可回放。

## 常见开发任务

- 更新契约：修改 soulseed-agi-core-models 后，需要同步调整其他 crate 引用并补充合同测试。
- Explain 字段对齐：确保任何新的降级或索引变动，都写入 indices_used / query_hash / degradation_reason。
- Thin-Waist 对接：实现真实客户端时，请遵守现有 trait 签名，保持 Append-Only 与 Explain 信息完整。
- HITL/Clarify 策略：若新增优先级或闸门逻辑，需更新 ACE、DFR 与 AuthZ 对应测试用例。

## 文档资源

- 01-SoulseedAGI开发架构文档/：需求源、设计基线、实施方案（含九种对话场景全景图）。
- docs/：补充 schema、事件定义、说明文档（例如 awareness_events_schema）。
- 根目录 src/ca.rs：ACE→CA 契约副本，便于外部团队在不进入 workspace 的情况下查阅接口。

## 下一步建议

1. 部署 Soulbase Gateway 与 SurrealDB 索引，完成 Thin-Waist 正式接入与索引初始化。
2. 搭建统一 workspace 或脚本，便于一次性运行全部测试、集成 CI。
3. 联调九场景脚本：在接入外部服务后，执行 Human↔AI、SelfReason、Collab 等端到端脚本，验证事件回放与 Explain 指纹。
4. 补充自动化：新增 indices_used/query_hash/degradation_reason 的一致性检查，逐步接入 burn-in replay 与性能监控。

---

如需进一步信息，可结合 开发清单.md 与架构文档了解各里程碑的背景与验收标准。

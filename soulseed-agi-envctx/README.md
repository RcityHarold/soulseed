# soulseed-agi-envctx

Soulseed 环境上下文 crate 按 RIS 规范实现了 Environment Context 的契约层：

- 统一 Anchor(六锚) 与 EnvironmentContext 数据结构。
- EnvironmentDataProvider trait 暴露了从 Graph / Observe / Policy / Tool Registry 拉取环境数据的端口。
- EnvironmentEngine 负责聚合、退化容忍、Explain 外泄与 canonical digest 计算。
- EnvCtxProvisioner 将 provider + reporter 封装为最小可运行装配器，配合同步/异步 runner 使用。
- SoulbaseEnvProvider + ThinWaistClient trait 提供了与 Soulbase Gateway 的正式接入形态。

## 与 Soulbase 的对接路线

当前仓库内仅提供了零副作用接口和内存 fallback：

1. Provider 适配器：EnvironmentDataProvider 需要由 Soulbase 的 Thin-Waist 客户端实现。
   - /graph.recall、/repo.append、/observe.emit、/policy.snapshot 等调用应通过 SB Gateway。
   - 现有单元测试使用 FakeProvider，正式接入时请替换为 REST/gRPC 适配器并保留接口签名。
2. 退化可观测性：DegradationReporter 目前默认 No-op。对接 Soulbase 后应落到 observe.emit，并附带 anchor + reason。
3. 环境快照事件：EnvironmentContext 已计算 context_digest，但尚未写回 DialogueEvent。完成 Soulbase 适配后，请在 ACE 写入 ENVIRONMENT_SNAPSHOT_EVENT 并携带该 digest。
4. 权限与配额：AuthZ/Quota 模块需在 Soulbase 侧新增 envctx/llm 相关资源，完成后应在 Context 层联调验证。

## 现阶段注意事项

- 测试用 fallback 数据仅用于合同验证，不应出现在生产路径。
- 若 EnvironmentDataProvider 返回 SourceUnavailable 或 Missing，Engine 会自动生成 fallback 值并记录退化原因，Explain 与指标会携带 envctx 前缀。
- 所有 digest 计算通过 soulseed-agi-envctx::compute_digest，便于后续回放比对。

## 下一步

- 实现 Soulbase 适配器(REST/gRPC) 并在 soulseed-agi-context 中注入。
- 在 ACE 写入 ENVIRONMENT_SNAPSHOT_EVENT 并串联 Explain / metrics。
- 与 AuthZ/Quota 同步 envctx 权限、配额策略，实现端到端验收。

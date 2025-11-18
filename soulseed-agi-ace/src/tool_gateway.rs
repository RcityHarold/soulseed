use crate::errors::AceError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

/// 工具网关 - 管理工具执行请求和生命周期
///
/// 负责：
/// 1. 工具调用路由
/// 2. 执行状态跟踪
/// 3. 并发控制
#[derive(Clone)]
pub struct ToolGateway {
    state: Arc<Mutex<GatewayState>>,
    config: ToolGatewayConfig,
}

struct GatewayState {
    /// 正在执行的工具调用
    executing: HashMap<String, ToolExecution>,
    /// 工具调用历史
    history: Vec<ToolExecutionRecord>,
    /// 每个工具的并发计数
    tool_concurrency: HashMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolGatewayConfig {
    /// 全局最大并发数
    pub max_concurrent: usize,
    /// 每个工具的最大并发数
    pub max_per_tool: usize,
    /// 执行超时（毫秒）
    pub timeout_ms: u64,
}

impl Default for ToolGatewayConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            max_per_tool: 3,
            timeout_ms: 60_000, // 60秒
        }
    }
}

#[derive(Clone, Debug)]
struct ToolExecution {
    _execution_id: String,
    tool_id: String,
    started_at: OffsetDateTime,
    status: ExecutionStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Timeout,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub execution_id: String,
    pub tool_id: String,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub status: ExecutionStatus,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub execution_id: String,
    pub tool_id: String,
    pub inputs: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

impl ToolGateway {
    pub fn new(config: ToolGatewayConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(GatewayState {
                executing: HashMap::new(),
                history: Vec::new(),
                tool_concurrency: HashMap::new(),
            })),
            config,
        }
    }

    /// 提交工具执行请求
    pub fn submit(&self, request: ToolExecutionRequest) -> Result<(), AceError> {
        let mut state = self.state.lock().unwrap();

        // 检查全局并发限制
        if state.executing.len() >= self.config.max_concurrent {
            return Err(AceError::Tool(format!(
                "Global concurrency limit reached: {}",
                self.config.max_concurrent
            )));
        }

        // 检查单工具并发限制
        let tool_count = state
            .tool_concurrency
            .get(&request.tool_id)
            .copied()
            .unwrap_or(0);
        if tool_count >= self.config.max_per_tool {
            return Err(AceError::Tool(format!(
                "Tool concurrency limit reached for {}: {}",
                request.tool_id, self.config.max_per_tool
            )));
        }

        // 记录执行
        let execution = ToolExecution {
            _execution_id: request.execution_id.clone(),
            tool_id: request.tool_id.clone(),
            started_at: OffsetDateTime::now_utc(),
            status: ExecutionStatus::Running,
        };

        state
            .executing
            .insert(request.execution_id.clone(), execution);
        *state
            .tool_concurrency
            .entry(request.tool_id.clone())
            .or_insert(0) += 1;

        Ok(())
    }

    /// 完成工具执行
    pub fn complete(
        &self,
        execution_id: &str,
        result: ToolExecutionResult,
    ) -> Result<(), AceError> {
        let mut state = self.state.lock().unwrap();

        let execution = state.executing.remove(execution_id).ok_or_else(|| {
            AceError::Tool(format!("Execution not found: {}", execution_id))
        })?;

        // 更新工具并发计数
        if let Some(count) = state.tool_concurrency.get_mut(&execution.tool_id) {
            *count = count.saturating_sub(1);
        }

        // 记录到历史
        state.history.push(ToolExecutionRecord {
            execution_id: execution_id.to_string(),
            tool_id: execution.tool_id,
            started_at: execution.started_at,
            completed_at: Some(OffsetDateTime::now_utc()),
            status: result.status,
            error: result.error,
        });

        Ok(())
    }

    /// 获取执行状态
    pub fn get_status(&self, execution_id: &str) -> Option<ExecutionStatus> {
        let state = self.state.lock().unwrap();
        state
            .executing
            .get(execution_id)
            .map(|exec| exec.status.clone())
    }

    /// 获取当前执行数量
    pub fn get_executing_count(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.executing.len()
    }

    /// 检查超时的执行
    pub fn check_timeouts(&self) -> Vec<String> {
        let mut state = self.state.lock().unwrap();
        let now = OffsetDateTime::now_utc();
        let timeout_duration = time::Duration::milliseconds(self.config.timeout_ms as i64);

        let mut timeout_ids = Vec::new();

        for (id, exec) in &mut state.executing {
            if now - exec.started_at > timeout_duration {
                exec.status = ExecutionStatus::Timeout;
                timeout_ids.push(id.clone());
            }
        }

        timeout_ids
    }
}

/// DAG执行器 - 管理有向无环图的工具执行
///
/// 支持：
/// 1. 依赖关系管理
/// 2. 并行执行
/// 3. 错误传播
#[derive(Clone)]
pub struct DagExecutor {
    gateway: ToolGateway,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagNode {
    pub node_id: String,
    pub tool_id: String,
    pub inputs: serde_json::Value,
    /// 依赖的节点ID列表
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagExecutionPlan {
    pub nodes: Vec<DagNode>,
}

#[derive(Clone, Debug)]
pub struct DagExecutionState {
    /// 已完成的节点
    completed: HashSet<String>,
    /// 正在执行的节点
    executing: HashSet<String>,
    /// 失败的节点
    failed: HashSet<String>,
    /// 节点结果
    results: HashMap<String, serde_json::Value>,
}

impl DagExecutor {
    pub fn new(gateway: ToolGateway) -> Self {
        Self { gateway }
    }

    /// 执行DAG计划
    pub async fn execute(&self, plan: DagExecutionPlan) -> Result<HashMap<String, serde_json::Value>, AceError> {
        let mut state = DagExecutionState {
            completed: HashSet::new(),
            executing: HashSet::new(),
            failed: HashSet::new(),
            results: HashMap::new(),
        };

        // 构建依赖图
        let dependencies = self.build_dependency_map(&plan);

        // 执行直到所有节点完成或失败
        while state.completed.len() + state.failed.len() < plan.nodes.len() {
            // 找到可以执行的节点（所有依赖都已完成）
            let ready_nodes: Vec<&DagNode> = plan
                .nodes
                .iter()
                .filter(|node| {
                    !state.completed.contains(&node.node_id)
                        && !state.executing.contains(&node.node_id)
                        && !state.failed.contains(&node.node_id)
                        && self.are_dependencies_met(node, &state, &dependencies)
                })
                .collect();

            if ready_nodes.is_empty() {
                // 如果没有ready的节点但还有未完成的节点，说明有循环依赖或失败
                if state.executing.is_empty() {
                    return Err(AceError::Tool(
                        "DAG execution deadlock: circular dependency or all paths failed".into(),
                    ));
                }
                // 等待执行中的节点完成
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            // 并发执行ready的节点
            for node in ready_nodes {
                self.execute_node(node, &mut state).await?;
            }
        }

        // 检查是否有失败的节点
        if !state.failed.is_empty() {
            return Err(AceError::Tool(format!(
                "DAG execution failed: {} nodes failed",
                state.failed.len()
            )));
        }

        Ok(state.results)
    }

    fn build_dependency_map(&self, plan: &DagExecutionPlan) -> HashMap<String, Vec<String>> {
        plan.nodes
            .iter()
            .map(|node| (node.node_id.clone(), node.dependencies.clone()))
            .collect()
    }

    fn are_dependencies_met(
        &self,
        node: &DagNode,
        state: &DagExecutionState,
        _dependencies: &HashMap<String, Vec<String>>,
    ) -> bool {
        node.dependencies
            .iter()
            .all(|dep| state.completed.contains(dep))
    }

    async fn execute_node(
        &self,
        node: &DagNode,
        state: &mut DagExecutionState,
    ) -> Result<(), AceError> {
        state.executing.insert(node.node_id.clone());

        let request = ToolExecutionRequest {
            execution_id: node.node_id.clone(),
            tool_id: node.tool_id.clone(),
            inputs: node.inputs.clone(),
        };

        // 提交执行
        self.gateway.submit(request)?;

        // 模拟执行（实际应该调用真实的工具服务）
        // 这里简化为立即完成
        let result = ToolExecutionResult {
            execution_id: node.node_id.clone(),
            status: ExecutionStatus::Completed,
            output: Some(serde_json::json!({"status": "success"})),
            error: None,
            duration_ms: 100,
        };

        self.gateway.complete(&node.node_id, result.clone())?;

        state.executing.remove(&node.node_id);
        state.completed.insert(node.node_id.clone());

        if let Some(output) = result.output {
            state.results.insert(node.node_id.clone(), output);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_gateway_submit() {
        let gateway = ToolGateway::new(ToolGatewayConfig::default());

        let request = ToolExecutionRequest {
            execution_id: "exec1".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        assert!(gateway.submit(request).is_ok());
        assert_eq!(gateway.get_executing_count(), 1);
    }

    #[test]
    fn test_tool_gateway_concurrency_limit() {
        let gateway = ToolGateway::new(ToolGatewayConfig {
            max_concurrent: 2,
            max_per_tool: 1,
            timeout_ms: 60_000,
        });

        let req1 = ToolExecutionRequest {
            execution_id: "exec1".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        let req2 = ToolExecutionRequest {
            execution_id: "exec2".to_string(),
            tool_id: "tool2".to_string(),
            inputs: serde_json::json!({}),
        };

        let req3 = ToolExecutionRequest {
            execution_id: "exec3".to_string(),
            tool_id: "tool3".to_string(),
            inputs: serde_json::json!({}),
        };

        assert!(gateway.submit(req1).is_ok());
        assert!(gateway.submit(req2).is_ok());
        assert!(gateway.submit(req3).is_err()); // 超过max_concurrent
    }

    #[test]
    fn test_tool_gateway_complete() {
        let gateway = ToolGateway::new(ToolGatewayConfig::default());

        let request = ToolExecutionRequest {
            execution_id: "exec1".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        gateway.submit(request).unwrap();

        let result = ToolExecutionResult {
            execution_id: "exec1".to_string(),
            status: ExecutionStatus::Completed,
            output: Some(serde_json::json!({"result": "success"})),
            error: None,
            duration_ms: 100,
        };

        assert!(gateway.complete("exec1", result).is_ok());
        assert_eq!(gateway.get_executing_count(), 0);
    }

    #[test]
    fn test_tool_gateway_per_tool_limit() {
        let gateway = ToolGateway::new(ToolGatewayConfig {
            max_concurrent: 10,
            max_per_tool: 2,
            timeout_ms: 60_000,
        });

        let req1 = ToolExecutionRequest {
            execution_id: "exec1".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        let req2 = ToolExecutionRequest {
            execution_id: "exec2".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        let req3 = ToolExecutionRequest {
            execution_id: "exec3".to_string(),
            tool_id: "tool1".to_string(),
            inputs: serde_json::json!({}),
        };

        assert!(gateway.submit(req1).is_ok());
        assert!(gateway.submit(req2).is_ok());
        assert!(gateway.submit(req3).is_err()); // 超过max_per_tool for tool1
    }

    #[tokio::test]
    async fn test_dag_executor_simple() {
        let gateway = ToolGateway::new(ToolGatewayConfig::default());
        let executor = DagExecutor::new(gateway);

        let plan = DagExecutionPlan {
            nodes: vec![
                DagNode {
                    node_id: "node1".to_string(),
                    tool_id: "tool1".to_string(),
                    inputs: serde_json::json!({}),
                    dependencies: vec![],
                },
                DagNode {
                    node_id: "node2".to_string(),
                    tool_id: "tool2".to_string(),
                    inputs: serde_json::json!({}),
                    dependencies: vec!["node1".to_string()],
                },
            ],
        };

        let results = executor.execute(plan).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
    }
}

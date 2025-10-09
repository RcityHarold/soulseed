use std::collections::{HashMap, HashSet};

use crate::types::RoutePlan;
use soulseed_agi_core_models::awareness::{
    ClarifyLimits, ClarifyPlan, CollabPlan, DecisionPlan, SelfPlan, ToolPlan, ToolPlanBarrier,
    ToolPlanEdge, ToolPlanNode,
};

pub fn validate_decision_plan(plan: &RoutePlan) -> Result<(), String> {
    match &plan.decision_plan {
        DecisionPlan::SelfReason { plan } => validate_self_plan(plan),
        DecisionPlan::Clarify { plan } => validate_clarify_plan(plan),
        DecisionPlan::Tool { plan } => validate_tool_plan(plan),
        DecisionPlan::Collab { plan } => validate_collab_plan(plan),
    }
}

fn validate_self_plan(plan: &SelfPlan) -> Result<(), String> {
    if plan.max_ic.map(|v| v == 0).unwrap_or(false) {
        return Err("self_max_ic_zero".into());
    }
    Ok(())
}

fn validate_clarify_plan(plan: &ClarifyPlan) -> Result<(), String> {
    for (idx, question) in plan.questions.iter().enumerate() {
        if question.q_id.trim().is_empty() {
            return Err(format!("clarify_question_missing_id:{}", idx));
        }
        if question.text.trim().is_empty() {
            return Err(format!("clarify_question_missing_text:{}", idx));
        }
    }
    validate_limits(&plan.limits, "clarify")?;
    Ok(())
}

fn validate_limits(limits: &ClarifyLimits, prefix: &str) -> Result<(), String> {
    if limits.max_rounds.map(|v| v == 0).unwrap_or(false) {
        return Err(format!("{}_limits_rounds_zero", prefix));
    }
    if limits.max_parallel.map(|v| v == 0).unwrap_or(false) {
        return Err(format!("{}_limits_parallel_zero", prefix));
    }
    if limits.wait_ms.map(|v| v == 0).unwrap_or(false) {
        return Err(format!("{}_limits_wait_zero", prefix));
    }
    if limits.total_wait_ms.map(|v| v == 0).unwrap_or(false) {
        return Err(format!("{}_limits_total_wait_zero", prefix));
    }
    Ok(())
}

fn validate_tool_plan(plan: &ToolPlan) -> Result<(), String> {
    if plan.nodes.is_empty() {
        return Err("tool_nodes_empty".into());
    }

    let mut seen = HashSet::new();
    for node in &plan.nodes {
        if node.id.trim().is_empty() {
            return Err("tool_node_missing_id".into());
        }
        if !seen.insert(node.id.clone()) {
            return Err(format!("tool_node_duplicate:{}", node.id));
        }
        if node.tool_id.trim().is_empty() {
            return Err(format!("tool_node_missing_tool_id:{}", node.id));
        }
        if node.timeout_ms.map(|v| v == 0).unwrap_or(false) {
            return Err(format!("tool_node_timeout_zero:{}", node.id));
        }
    }

    let valid_nodes: HashSet<_> = plan.nodes.iter().map(|n| n.id.as_str()).collect();
    for edge in &plan.edges {
        if !valid_nodes.contains(edge.from.as_str()) {
            return Err(format!("tool_edge_unknown_from:{}->{}", edge.from, edge.to));
        }
        if !valid_nodes.contains(edge.to.as_str()) {
            return Err(format!("tool_edge_unknown_to:{}->{}", edge.from, edge.to));
        }
    }

    if has_cycle(&plan.nodes, &plan.edges) {
        return Err("tool_dag_cycle".into());
    }

    validate_barrier(&plan.barrier)?;
    Ok(())
}

fn validate_barrier(barrier: &ToolPlanBarrier) -> Result<(), String> {
    if let Some(mode) = barrier.mode.as_deref() {
        const VALID: [&str; 4] = ["all", "any", "sequence", "quorum"];
        if !VALID.contains(&mode) {
            return Err(format!("tool_barrier_mode_invalid:{}", mode));
        }
    }
    if barrier.timeout_ms.map(|v| v == 0).unwrap_or(false) {
        return Err("tool_barrier_timeout_zero".into());
    }
    Ok(())
}

fn has_cycle(nodes: &[ToolPlanNode], edges: &[ToolPlanEdge]) -> bool {
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in nodes {
        graph.entry(node.id.as_str()).or_default();
    }
    for edge in edges {
        graph
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    fn dfs(
        node: &str,
        graph: &HashMap<&str, Vec<&str>>,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> bool {
        if stack.contains(node) {
            return true;
        }
        if !visited.insert(node.to_string()) {
            return false;
        }
        stack.insert(node.to_string());
        if let Some(neighbors) = graph.get(node) {
            for &neighbor in neighbors {
                if dfs(neighbor, graph, visited, stack) {
                    return true;
                }
            }
        }
        stack.remove(node);
        false
    }

    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    for node in graph.keys() {
        if dfs(node, &graph, &mut visited, &mut stack) {
            return true;
        }
    }
    false
}

fn validate_collab_plan(plan: &CollabPlan) -> Result<(), String> {
    if plan.scope.is_null() {
        return Err("collab_scope_missing".into());
    }
    if let Some(rounds) = plan.rounds {
        if rounds == 0 {
            return Err("collab_rounds_zero".into());
        }
    }
    if let Some(mode) = plan.privacy_mode.as_deref() {
        const VALID: [&str; 3] = ["shared", "minimum", "evidence_only"];
        if !VALID.contains(&mode) {
            return Err(format!("collab_privacy_invalid:{}", mode));
        }
    }
    validate_barrier(&plan.barrier)?;
    Ok(())
}

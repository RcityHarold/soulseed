//! 决策指纹系统
//!
//! 用于计算和匹配决策上下文的指纹，支持历史决策查询和相似度匹配。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulseed_agi_core_models::awareness::DecisionPath;
use std::collections::HashMap;

/// 特征类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum Feature {
    /// 数值特征
    Numeric(f32),
    /// 分类特征
    Categorical(String),
    /// 布尔特征
    Boolean(bool),
    /// 向量特征（用于嵌入）
    Vector(Vec<f32>),
    /// 文本特征
    Text(String),
}

impl Feature {
    pub fn numeric(value: f32) -> Self {
        Self::Numeric(value)
    }

    pub fn categorical(value: impl Into<String>) -> Self {
        Self::Categorical(value.into())
    }

    pub fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    pub fn vector(value: Vec<f32>) -> Self {
        Self::Vector(value)
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }
}

/// 输入特征集合
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct InputFeatures {
    /// 特征名到特征值的映射
    pub features: HashMap<String, Feature>,
    /// 特征权重（用于匹配时的加权计算）
    #[serde(default)]
    pub weights: HashMap<String, f32>,
}

impl InputFeatures {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加特征
    pub fn add(&mut self, name: impl Into<String>, feature: Feature) {
        self.features.insert(name.into(), feature);
    }

    /// 添加带权重的特征
    pub fn add_weighted(&mut self, name: impl Into<String>, feature: Feature, weight: f32) {
        let name = name.into();
        self.features.insert(name.clone(), feature);
        self.weights.insert(name, weight);
    }

    /// 获取特征
    pub fn get(&self, name: &str) -> Option<&Feature> {
        self.features.get(name)
    }

    /// 获取特征权重（默认为 1.0）
    pub fn weight(&self, name: &str) -> f32 {
        self.weights.get(name).copied().unwrap_or(1.0)
    }

    /// 计算与另一个特征集的相似度
    pub fn similarity(&self, other: &InputFeatures) -> f32 {
        if self.features.is_empty() || other.features.is_empty() {
            return 0.0;
        }

        let mut total_weight = 0.0f32;
        let mut weighted_similarity = 0.0f32;

        for (name, feature) in &self.features {
            if let Some(other_feature) = other.features.get(name) {
                let weight = self.weight(name);
                let sim = feature_similarity(feature, other_feature);
                weighted_similarity += sim * weight;
                total_weight += weight;
            }
        }

        if total_weight > 0.0 {
            weighted_similarity / total_weight
        } else {
            0.0
        }
    }
}

/// 计算两个特征的相似度
fn feature_similarity(a: &Feature, b: &Feature) -> f32 {
    match (a, b) {
        (Feature::Numeric(va), Feature::Numeric(vb)) => {
            // 使用高斯相似度
            let diff = (va - vb).abs();
            (-diff * diff / 2.0).exp()
        }
        (Feature::Categorical(va), Feature::Categorical(vb)) => {
            if va == vb { 1.0 } else { 0.0 }
        }
        (Feature::Boolean(va), Feature::Boolean(vb)) => {
            if va == vb { 1.0 } else { 0.0 }
        }
        (Feature::Vector(va), Feature::Vector(vb)) => {
            cosine_similarity(va, vb)
        }
        (Feature::Text(va), Feature::Text(vb)) => {
            // 简单的 Jaccard 相似度（基于词）
            text_similarity(va, vb)
        }
        _ => 0.0, // 不同类型的特征相似度为 0
    }
}

/// 计算余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

/// 计算文本相似度（简单的词级 Jaccard）
fn text_similarity(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union > 0 {
        intersection as f32 / union as f32
    } else {
        0.0
    }
}

/// 决策指纹
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionFingerprint {
    /// 指纹唯一标识
    pub fingerprint_id: String,
    /// 上下文哈希（用于快速比对）
    pub context_hash: String,
    /// 输入特征集合
    pub input_features: InputFeatures,
    /// 决策路径
    pub decision_path: DecisionPath,
    /// 决策置信度
    pub confidence: f32,
    /// 创建时间戳（毫秒）
    pub created_at_ms: i64,
    /// 关联的 AC ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac_id: Option<String>,
    /// 关联的 Session ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// 决策结果（成功/失败/未知）
    #[serde(default)]
    pub outcome: FingerprintOutcome,
    /// 额外元数据
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

/// 指纹结果状态
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum FingerprintOutcome {
    #[default]
    Unknown,
    Success,
    Failure,
    Partial,
}

impl DecisionFingerprint {
    /// 创建新的决策指纹
    pub fn new(
        context_hash: impl Into<String>,
        input_features: InputFeatures,
        decision_path: DecisionPath,
        confidence: f32,
    ) -> Self {
        let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let fingerprint_id = format!("fp-{}", uuid::Uuid::new_v4());

        Self {
            fingerprint_id,
            context_hash: context_hash.into(),
            input_features,
            decision_path,
            confidence,
            created_at_ms: now_ms,
            ac_id: None,
            session_id: None,
            outcome: FingerprintOutcome::Unknown,
            metadata: Value::Null,
        }
    }

    /// 设置 AC ID
    pub fn with_ac_id(mut self, ac_id: impl Into<String>) -> Self {
        self.ac_id = Some(ac_id.into());
        self
    }

    /// 设置 Session ID
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 设置结果
    pub fn with_outcome(mut self, outcome: FingerprintOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// 计算与另一个指纹的相似度
    pub fn similarity(&self, other: &DecisionFingerprint) -> f32 {
        // 上下文哈希完全匹配时返回 1.0
        if self.context_hash == other.context_hash {
            return 1.0;
        }

        // 否则基于输入特征计算相似度
        self.input_features.similarity(&other.input_features)
    }
}

/// 指纹匹配结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FingerprintMatch {
    /// 匹配到的指纹
    pub fingerprint: DecisionFingerprint,
    /// 相似度分数 (0.0 - 1.0)
    pub similarity: f32,
    /// 是否为精确匹配（上下文哈希相同）
    pub exact_match: bool,
    /// 匹配的特征列表
    pub matched_features: Vec<String>,
}

/// 决策上下文（用于计算指纹）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionContext {
    /// 用户意图
    pub user_intent: Option<String>,
    /// 场景类型
    pub scenario: Option<String>,
    /// 上下文信号
    pub context_signals: Vec<(String, f32)>,
    /// 当前活跃的工具
    pub active_tools: Vec<String>,
    /// 对话历史长度
    pub history_length: usize,
    /// 其他上下文数据
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub extra: Value,
}

impl Default for DecisionContext {
    fn default() -> Self {
        Self {
            user_intent: None,
            scenario: None,
            context_signals: Vec::new(),
            active_tools: Vec::new(),
            history_length: 0,
            extra: Value::Null,
        }
    }
}

impl DecisionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.user_intent = Some(intent.into());
        self
    }

    pub fn with_scenario(mut self, scenario: impl Into<String>) -> Self {
        self.scenario = Some(scenario.into());
        self
    }

    pub fn with_signals(mut self, signals: Vec<(String, f32)>) -> Self {
        self.context_signals = signals;
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.active_tools = tools;
        self
    }
}

/// 计算决策上下文的指纹
pub fn compute_fingerprint(
    context: &DecisionContext,
    decision_path: DecisionPath,
    confidence: f32,
) -> DecisionFingerprint {
    // 计算上下文哈希
    let context_hash = compute_context_hash(context);

    // 提取输入特征
    let mut features = InputFeatures::new();

    // 添加意图特征
    if let Some(ref intent) = context.user_intent {
        features.add_weighted("user_intent", Feature::text(intent.clone()), 2.0);
    }

    // 添加场景特征
    if let Some(ref scenario) = context.scenario {
        features.add_weighted("scenario", Feature::categorical(scenario.clone()), 1.5);
    }

    // 添加上下文信号特征
    for (name, score) in &context.context_signals {
        features.add(format!("signal_{}", name), Feature::numeric(*score));
    }

    // 添加历史长度特征
    features.add("history_length", Feature::numeric(context.history_length as f32));

    // 添加活跃工具数量特征
    features.add("active_tools_count", Feature::numeric(context.active_tools.len() as f32));

    DecisionFingerprint::new(context_hash, features, decision_path, confidence)
}

/// 计算上下文哈希
fn compute_context_hash(context: &DecisionContext) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // 哈希意图
    if let Some(ref intent) = context.user_intent {
        intent.hash(&mut hasher);
    }

    // 哈希场景
    if let Some(ref scenario) = context.scenario {
        scenario.hash(&mut hasher);
    }

    // 哈希信号（排序后）
    let mut signals: Vec<_> = context.context_signals.iter().collect();
    signals.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, _) in signals {
        name.hash(&mut hasher);
    }

    // 哈希工具（排序后）
    let mut tools = context.active_tools.clone();
    tools.sort();
    for tool in &tools {
        tool.hash(&mut hasher);
    }

    format!("ctx-{:016x}", hasher.finish())
}

/// 在历史指纹中查找匹配
pub fn match_fingerprint(
    current: &DecisionFingerprint,
    historical: &[DecisionFingerprint],
    threshold: f32,
) -> Vec<FingerprintMatch> {
    let mut matches: Vec<FingerprintMatch> = historical
        .iter()
        .filter_map(|h| {
            let similarity = current.similarity(h);
            if similarity >= threshold {
                let exact_match = current.context_hash == h.context_hash;
                let matched_features: Vec<String> = current
                    .input_features
                    .features
                    .keys()
                    .filter(|k| h.input_features.features.contains_key(*k))
                    .cloned()
                    .collect();

                Some(FingerprintMatch {
                    fingerprint: h.clone(),
                    similarity,
                    exact_match,
                    matched_features,
                })
            } else {
                None
            }
        })
        .collect();

    // 按相似度降序排序
    matches.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

    matches
}

/// 指纹存储 trait
pub trait FingerprintStore {
    /// 存储指纹
    fn store(&mut self, fingerprint: DecisionFingerprint);

    /// 根据 ID 获取指纹
    fn get(&self, fingerprint_id: &str) -> Option<&DecisionFingerprint>;

    /// 查找匹配的指纹
    fn find_matches(&self, current: &DecisionFingerprint, threshold: f32) -> Vec<FingerprintMatch>;

    /// 更新指纹结果
    fn update_outcome(&mut self, fingerprint_id: &str, outcome: FingerprintOutcome);

    /// 获取指定 Session 的所有指纹
    fn get_by_session(&self, session_id: &str) -> Vec<&DecisionFingerprint>;

    /// 清理过期指纹
    fn cleanup(&mut self, max_age_ms: i64);
}

/// 内存中的指纹存储实现
#[derive(Default)]
pub struct InMemoryFingerprintStore {
    fingerprints: HashMap<String, DecisionFingerprint>,
    by_session: HashMap<String, Vec<String>>,
}

impl InMemoryFingerprintStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.fingerprints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fingerprints.is_empty()
    }
}

impl FingerprintStore for InMemoryFingerprintStore {
    fn store(&mut self, fingerprint: DecisionFingerprint) {
        let id = fingerprint.fingerprint_id.clone();

        // 更新 session 索引
        if let Some(ref session_id) = fingerprint.session_id {
            self.by_session
                .entry(session_id.clone())
                .or_default()
                .push(id.clone());
        }

        self.fingerprints.insert(id, fingerprint);
    }

    fn get(&self, fingerprint_id: &str) -> Option<&DecisionFingerprint> {
        self.fingerprints.get(fingerprint_id)
    }

    fn find_matches(&self, current: &DecisionFingerprint, threshold: f32) -> Vec<FingerprintMatch> {
        let historical: Vec<_> = self.fingerprints.values().cloned().collect();
        match_fingerprint(current, &historical, threshold)
    }

    fn update_outcome(&mut self, fingerprint_id: &str, outcome: FingerprintOutcome) {
        if let Some(fp) = self.fingerprints.get_mut(fingerprint_id) {
            fp.outcome = outcome;
        }
    }

    fn get_by_session(&self, session_id: &str) -> Vec<&DecisionFingerprint> {
        self.by_session
            .get(session_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.fingerprints.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn cleanup(&mut self, max_age_ms: i64) {
        let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let cutoff = now_ms - max_age_ms;

        // 收集过期的 ID
        let expired: Vec<String> = self.fingerprints
            .iter()
            .filter(|(_, fp)| fp.created_at_ms < cutoff)
            .map(|(id, _)| id.clone())
            .collect();

        // 删除过期指纹
        for id in expired {
            if let Some(fp) = self.fingerprints.remove(&id) {
                // 更新 session 索引
                if let Some(ref session_id) = fp.session_id {
                    if let Some(ids) = self.by_session.get_mut(session_id) {
                        ids.retain(|i| i != &id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::awareness::{
        AwarenessAnchor, AwarenessFork, DecisionPlan, SelfPlan,
        DecisionBudgetEstimate, DecisionExplain,
    };
    use soulseed_agi_core_models::common::AccessClass;
    use soulseed_agi_core_models::{TenantId, AwarenessCycleId, EnvelopeId};

    /// 创建测试用的 DecisionPath
    fn create_test_decision_path(fork: AwarenessFork) -> DecisionPath {
        DecisionPath {
            anchor: AwarenessAnchor {
                tenant_id: TenantId::from_raw_unchecked(1),
                envelope_id: uuid::Uuid::new_v4(),
                config_snapshot_hash: "test-hash".to_string(),
                config_snapshot_version: 1,
                session_id: None,
                sequence_number: None,
                access_class: AccessClass::Restricted,
                provenance: None,
                schema_v: 1,
            },
            awareness_cycle_id: AwarenessCycleId::generate(),
            inference_cycle_sequence: 1,
            fork,
            plan: DecisionPlan::SelfReason {
                plan: SelfPlan { hint: None, max_ic: None },
            },
            budget_plan: DecisionBudgetEstimate::default(),
            rationale: soulseed_agi_core_models::awareness::DecisionRationale::default(),
            confidence: 0.8,
            explain: DecisionExplain {
                routing_seed: 0,
                router_digest: "test".to_string(),
                router_config_digest: "test".to_string(),
                features_snapshot: None,
            },
            degradation_reason: None,
        }
    }

    #[test]
    fn test_feature_similarity_numeric() {
        assert!((feature_similarity(&Feature::Numeric(1.0), &Feature::Numeric(1.0)) - 1.0).abs() < 0.001);
        assert!(feature_similarity(&Feature::Numeric(0.0), &Feature::Numeric(10.0)) < 0.1);
    }

    #[test]
    fn test_feature_similarity_categorical() {
        assert_eq!(feature_similarity(&Feature::Categorical("a".into()), &Feature::Categorical("a".into())), 1.0);
        assert_eq!(feature_similarity(&Feature::Categorical("a".into()), &Feature::Categorical("b".into())), 0.0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001);
    }

    #[test]
    fn test_text_similarity() {
        assert!((text_similarity("hello world", "hello world") - 1.0).abs() < 0.001);
        assert!(text_similarity("hello world", "goodbye moon") < 0.5);
        assert!(text_similarity("hello world", "hello there") > 0.0);
    }

    #[test]
    fn test_input_features_similarity() {
        let mut f1 = InputFeatures::new();
        f1.add("intent", Feature::categorical("search"));
        f1.add("confidence", Feature::numeric(0.9));

        let mut f2 = InputFeatures::new();
        f2.add("intent", Feature::categorical("search"));
        f2.add("confidence", Feature::numeric(0.85));

        let sim = f1.similarity(&f2);
        assert!(sim > 0.5);
    }

    #[test]
    fn test_compute_fingerprint() {
        let context = DecisionContext::new()
            .with_intent("search for documents")
            .with_scenario("AiToSystem")
            .with_signals(vec![("clarity".into(), 0.8)]);

        let path = create_test_decision_path(AwarenessFork::ToolPath);
        let fp = compute_fingerprint(&context, path, 0.9);

        assert!(fp.context_hash.starts_with("ctx-"));
        assert!(fp.confidence == 0.9);
        assert!(fp.input_features.features.contains_key("user_intent"));
    }

    #[test]
    fn test_fingerprint_store() {
        let mut store = InMemoryFingerprintStore::new();

        let context = DecisionContext::new()
            .with_intent("test")
            .with_scenario("Test");

        let path = create_test_decision_path(AwarenessFork::SelfReason);
        let mut fp = compute_fingerprint(&context, path, 0.8);
        fp.session_id = Some("session-1".into());

        let fp_id = fp.fingerprint_id.clone();
        store.store(fp);

        assert_eq!(store.len(), 1);
        assert!(store.get(&fp_id).is_some());

        store.update_outcome(&fp_id, FingerprintOutcome::Success);
        assert_eq!(store.get(&fp_id).unwrap().outcome, FingerprintOutcome::Success);

        let by_session = store.get_by_session("session-1");
        assert_eq!(by_session.len(), 1);
    }

    #[test]
    fn test_match_fingerprint() {
        let context1 = DecisionContext::new()
            .with_intent("search documents")
            .with_scenario("AiToSystem");

        let context2 = DecisionContext::new()
            .with_intent("search documents")
            .with_scenario("AiToSystem");

        let context3 = DecisionContext::new()
            .with_intent("completely different")
            .with_scenario("HumanToAi");

        let path1 = create_test_decision_path(AwarenessFork::ToolPath);
        let path2 = create_test_decision_path(AwarenessFork::ToolPath);
        let path3 = create_test_decision_path(AwarenessFork::SelfReason);

        let fp1 = compute_fingerprint(&context1, path1, 0.9);
        let fp2 = compute_fingerprint(&context2, path2, 0.85);
        let fp3 = compute_fingerprint(&context3, path3, 0.7);

        let historical = vec![fp2, fp3];
        let matches = match_fingerprint(&fp1, &historical, 0.5);

        // fp1 和 fp2 应该匹配（相同的上下文哈希）
        assert!(!matches.is_empty());
        assert!(matches[0].exact_match);
    }
}

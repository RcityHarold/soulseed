use crate::errors::ContextError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 向量索引 - 用于ANN（Approximate Nearest Neighbor）检索
///
/// 支持向量相似度检索，用于语义相似的上下文查找。
#[derive(Clone, Debug)]
pub struct VectorIndex {
    /// 存储每个context item的向量表示
    vectors: HashMap<String, Vec<f32>>,
    /// 向量维度
    dimension: usize,
    /// 索引配置
    config: VectorIndexConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorIndexConfig {
    /// 向量维度
    pub dimension: usize,
    /// 距离度量方式 (cosine, euclidean, dot_product)
    pub distance_metric: DistanceMetric,
    /// 是否启用归一化
    pub normalize: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl Default for VectorIndexConfig {
    fn default() -> Self {
        Self {
            dimension: 768, // 默认使用768维（如BERT）
            distance_metric: DistanceMetric::Cosine,
            normalize: true,
        }
    }
}

impl VectorIndex {
    pub fn new(config: VectorIndexConfig) -> Self {
        Self {
            vectors: HashMap::new(),
            dimension: config.dimension,
            config,
        }
    }

    /// 添加向量到索引
    pub fn add(&mut self, item_id: String, vector: Vec<f32>) -> Result<(), ContextError> {
        if vector.len() != self.dimension {
            return Err(ContextError::InvalidData(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            )));
        }

        let normalized = if self.config.normalize {
            Self::normalize_vector(&vector)
        } else {
            vector
        };

        self.vectors.insert(item_id, normalized);
        Ok(())
    }

    /// 查找最相似的k个向量
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, ContextError> {
        if query.len() != self.dimension {
            return Err(ContextError::InvalidData(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let query_vec = if self.config.normalize {
            Self::normalize_vector(&query.to_vec())
        } else {
            query.to_vec()
        };

        let mut results: Vec<SearchResult> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let distance = self.compute_distance(&query_vec, vec);
                SearchResult {
                    item_id: id.clone(),
                    distance,
                }
            })
            .collect();

        // 按距离排序（距离越小越相似，除了dot_product）
        results.sort_by(|a, b| {
            if matches!(self.config.distance_metric, DistanceMetric::DotProduct) {
                b.distance.partial_cmp(&a.distance).unwrap()
            } else {
                a.distance.partial_cmp(&b.distance).unwrap()
            }
        });

        Ok(results.into_iter().take(k).collect())
    }

    /// 归一化向量
    fn normalize_vector(vec: &[f32]) -> Vec<f32> {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            vec.iter().map(|x| x / norm).collect()
        } else {
            vec.to_vec()
        }
    }

    /// 计算两个向量之间的距离
    fn compute_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.config.distance_metric {
            DistanceMetric::Cosine => {
                // Cosine距离 = 1 - cosine相似度
                let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
                1.0 - dot_product
            }
            DistanceMetric::Euclidean => {
                // 欧氏距离
                a.iter()
                    .zip(b.iter())
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f32>()
                    .sqrt()
            }
            DistanceMetric::DotProduct => {
                // 点积（越大越相似）
                a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
            }
        }
    }

    /// 删除向量
    pub fn remove(&mut self, item_id: &str) -> bool {
        self.vectors.remove(item_id).is_some()
    }

    /// 获取索引大小
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// 检查索引是否为空
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

/// 稀疏索引 - 倒排索引，用于关键词检索
///
/// 支持BM25等传统检索算法。
#[derive(Clone, Debug)]
pub struct SparseIndex {
    /// 倒排表: term -> [(item_id, tf)]
    inverted_index: HashMap<String, Vec<(String, f32)>>,
    /// 文档频率: term -> df
    document_frequency: HashMap<String, usize>,
    /// 文档总数
    total_documents: usize,
    /// 文档长度: item_id -> length
    document_lengths: HashMap<String, usize>,
    /// 平均文档长度
    avg_doc_length: f32,
}

impl Default for SparseIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SparseIndex {
    pub fn new() -> Self {
        Self {
            inverted_index: HashMap::new(),
            document_frequency: HashMap::new(),
            total_documents: 0,
            document_lengths: HashMap::new(),
            avg_doc_length: 0.0,
        }
    }

    /// 添加文档到索引
    pub fn add(&mut self, item_id: String, tokens: Vec<String>) {
        // 计算词频
        let mut term_freq: HashMap<String, f32> = HashMap::new();
        for token in &tokens {
            *term_freq.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        // 更新倒排索引
        for (term, tf) in term_freq {
            self.inverted_index
                .entry(term.clone())
                .or_insert_with(Vec::new)
                .push((item_id.clone(), tf));

            // 更新文档频率
            *self.document_frequency.entry(term).or_insert(0) += 1;
        }

        // 更新文档长度
        self.document_lengths.insert(item_id, tokens.len());
        self.total_documents += 1;

        // 更新平均文档长度
        let total_length: usize = self.document_lengths.values().sum();
        self.avg_doc_length = total_length as f32 / self.total_documents as f32;
    }

    /// BM25检索
    pub fn search_bm25(&self, query_tokens: &[String], k: usize) -> Vec<SearchResult> {
        let k1 = 1.5; // BM25参数
        let b = 0.75; // BM25参数

        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in query_tokens {
            if let Some(postings) = self.inverted_index.get(term) {
                let df = *self.document_frequency.get(term).unwrap_or(&1);
                let idf = ((self.total_documents as f32 - df as f32 + 0.5)
                    / (df as f32 + 0.5)
                    + 1.0)
                    .ln();

                for (doc_id, tf) in postings {
                    let doc_len = *self.document_lengths.get(doc_id).unwrap_or(&1) as f32;
                    let norm = 1.0 - b + b * (doc_len / self.avg_doc_length);
                    let score = idf * ((tf * (k1 + 1.0)) / (tf + k1 * norm));
                    *scores.entry(doc_id.clone()).or_insert(0.0) += score;
                }
            }
        }

        let mut results: Vec<SearchResult> = scores
            .into_iter()
            .map(|(item_id, distance)| SearchResult {
                item_id,
                distance: -distance, // 负分数用于排序
            })
            .collect();

        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        results.into_iter().take(k).collect()
    }

    /// 删除文档
    pub fn remove(&mut self, item_id: &str) -> bool {
        // 从倒排索引中移除
        let mut removed = false;
        for postings in self.inverted_index.values_mut() {
            postings.retain(|(id, _)| id != item_id);
            removed = true;
        }

        // 移除文档长度记录
        if self.document_lengths.remove(item_id).is_some() {
            self.total_documents = self.total_documents.saturating_sub(1);
            // 重新计算平均文档长度
            if self.total_documents > 0 {
                let total_length: usize = self.document_lengths.values().sum();
                self.avg_doc_length = total_length as f32 / self.total_documents as f32;
            } else {
                self.avg_doc_length = 0.0;
            }
            removed = true;
        }

        removed
    }

    /// 获取索引大小
    pub fn len(&self) -> usize {
        self.total_documents
    }

    /// 检查索引是否为空
    pub fn is_empty(&self) -> bool {
        self.total_documents == 0
    }
}

/// 因果索引 - 因果关系图索引
///
/// 用于追踪上下文之间的因果依赖关系。
#[derive(Clone, Debug)]
pub struct CausalIndex {
    /// 因果关系图: item_id -> [dependent_item_ids]
    causal_graph: HashMap<String, HashSet<String>>,
    /// 反向因果图: item_id -> [causing_item_ids]
    reverse_graph: HashMap<String, HashSet<String>>,
}

impl Default for CausalIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl CausalIndex {
    pub fn new() -> Self {
        Self {
            causal_graph: HashMap::new(),
            reverse_graph: HashMap::new(),
        }
    }

    /// 添加因果关系：cause -> effect
    pub fn add_causal_link(&mut self, cause: String, effect: String) {
        // 添加到正向图
        self.causal_graph
            .entry(cause.clone())
            .or_insert_with(HashSet::new)
            .insert(effect.clone());

        // 添加到反向图
        self.reverse_graph
            .entry(effect)
            .or_insert_with(HashSet::new)
            .insert(cause);
    }

    /// 获取某个item直接导致的所有效果
    pub fn get_effects(&self, cause: &str) -> Vec<String> {
        self.causal_graph
            .get(cause)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 获取某个item的所有原因
    pub fn get_causes(&self, effect: &str) -> Vec<String> {
        self.reverse_graph
            .get(effect)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 获取因果链：从某个item开始的所有传递依赖
    pub fn get_causal_chain(&self, start: &str, max_depth: usize) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_effects(start, &mut visited, &mut result, 0, max_depth);
        result
    }

    fn dfs_effects(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth >= max_depth || visited.contains(node) {
            return;
        }

        visited.insert(node.to_string());
        result.push(node.to_string());

        if let Some(effects) = self.causal_graph.get(node) {
            for effect in effects {
                self.dfs_effects(effect, visited, result, depth + 1, max_depth);
            }
        }
    }

    /// 获取反向因果链：追溯到某个item的所有原因
    pub fn get_reverse_chain(&self, start: &str, max_depth: usize) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_causes(start, &mut visited, &mut result, 0, max_depth);
        result
    }

    fn dfs_causes(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth >= max_depth || visited.contains(node) {
            return;
        }

        visited.insert(node.to_string());
        result.push(node.to_string());

        if let Some(causes) = self.reverse_graph.get(node) {
            for cause in causes {
                self.dfs_causes(cause, visited, result, depth + 1, max_depth);
            }
        }
    }

    /// 删除某个节点及其所有因果关系
    pub fn remove(&mut self, item_id: &str) -> bool {
        let mut removed = false;

        // 从正向图中移除
        if self.causal_graph.remove(item_id).is_some() {
            removed = true;
        }

        // 从反向图中移除
        if self.reverse_graph.remove(item_id).is_some() {
            removed = true;
        }

        // 从其他节点的边中移除
        for edges in self.causal_graph.values_mut() {
            edges.remove(item_id);
        }

        for edges in self.reverse_graph.values_mut() {
            edges.remove(item_id);
        }

        removed
    }

    /// 获取索引大小
    pub fn len(&self) -> usize {
        self.causal_graph.len()
    }

    /// 检查索引是否为空
    pub fn is_empty(&self) -> bool {
        self.causal_graph.is_empty()
    }
}

/// 检索结果
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub item_id: String,
    pub distance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_index_add_and_search() {
        let mut index = VectorIndex::new(VectorIndexConfig {
            dimension: 3,
            distance_metric: DistanceMetric::Cosine,
            normalize: true,
        });

        index.add("item1".into(), vec![1.0, 0.0, 0.0]).unwrap();
        index.add("item2".into(), vec![0.0, 1.0, 0.0]).unwrap();
        index.add("item3".into(), vec![0.7, 0.7, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].item_id, "item1");
    }

    #[test]
    fn test_sparse_index_bm25() {
        let mut index = SparseIndex::new();

        index.add("doc1".into(), vec!["hello".into(), "world".into()]);
        index.add("doc2".into(), vec!["hello".into(), "rust".into()]);
        index.add("doc3".into(), vec!["world".into(), "rust".into()]);

        let results = index.search_bm25(&["hello".into()], 2);
        assert!(results.len() <= 2);
        // doc1 and doc2应该有更高的分数因为它们都包含"hello"
    }

    #[test]
    fn test_causal_index_chain() {
        let mut index = CausalIndex::new();

        index.add_causal_link("A".into(), "B".into());
        index.add_causal_link("B".into(), "C".into());
        index.add_causal_link("C".into(), "D".into());

        let chain = index.get_causal_chain("A", 10);
        assert!(chain.contains(&"A".to_string()));
        assert!(chain.contains(&"B".to_string()));
        assert!(chain.contains(&"C".to_string()));
        assert!(chain.contains(&"D".to_string()));

        let reverse = index.get_reverse_chain("D", 10);
        assert!(reverse.contains(&"D".to_string()));
        assert!(reverse.contains(&"C".to_string()));
        assert!(reverse.contains(&"B".to_string()));
        assert!(reverse.contains(&"A".to_string()));
    }

    #[test]
    fn test_vector_index_dimension_mismatch() {
        let mut index = VectorIndex::new(VectorIndexConfig {
            dimension: 3,
            ..Default::default()
        });

        let result = index.add("item1".into(), vec![1.0, 0.0]); // Wrong dimension
        assert!(result.is_err());
    }

    #[test]
    fn test_sparse_index_remove() {
        let mut index = SparseIndex::new();
        index.add("doc1".into(), vec!["hello".into(), "world".into()]);
        assert_eq!(index.len(), 1);

        index.remove("doc1");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_causal_index_effects_and_causes() {
        let mut index = CausalIndex::new();
        index.add_causal_link("A".into(), "B".into());
        index.add_causal_link("A".into(), "C".into());

        let effects = index.get_effects("A");
        assert_eq!(effects.len(), 2);
        assert!(effects.contains(&"B".to_string()));
        assert!(effects.contains(&"C".to_string()));

        let causes = index.get_causes("B");
        assert_eq!(causes.len(), 1);
        assert!(causes.contains(&"A".to_string()));
    }
}

//! 版本链管理器
//!
//! 依据文档: 6-元认知工具/1-元认知工具-2/01-对话事件类型以及元认知工具.md
//!
//! SupersedesChainManager 负责管理事件和会话的版本演化关系：
//! - 创建版本链（supersedes 关系）
//! - 追溯版本历史
//! - 查询最新版本
//! - 检测版本冲突

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// 版本链条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionChainEntry {
    /// 实体ID (事件ID或会话ID)
    pub entity_id: String,
    /// 实体类型
    pub entity_type: VersionedEntityType,
    /// 前驱版本ID (被此版本取代的版本)
    pub supersedes: Option<String>,
    /// 后继版本ID (取代此版本的版本)
    pub superseded_by: Option<String>,
    /// 版本号
    pub version: u32,
    /// 创建时间戳
    pub created_at_ms: i64,
    /// 版本链ID (同一版本链共享)
    pub chain_id: String,
    /// 是否为当前活动版本
    pub is_current: bool,
    /// 版本说明
    pub description: Option<String>,
}

/// 可版本化的实体类型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VersionedEntityType {
    /// 对话事件
    DialogueEvent,
    /// 会话
    Session,
    /// 消息
    Message,
    /// 工件
    Artifact,
}

/// 版本链摘要
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionChainSummary {
    /// 链ID
    pub chain_id: String,
    /// 实体类型
    pub entity_type: VersionedEntityType,
    /// 链长度
    pub chain_length: u32,
    /// 根版本ID
    pub root_id: String,
    /// 当前版本ID
    pub current_id: String,
    /// 所有版本ID列表（从旧到新）
    pub version_ids: Vec<String>,
    /// 创建时间
    pub created_at_ms: i64,
    /// 最后更新时间
    pub updated_at_ms: i64,
}

/// 版本冲突类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersionConflictType {
    /// 循环引用
    CyclicReference,
    /// 重复取代
    DuplicateSupersession,
    /// 孤立版本
    OrphanVersion,
    /// 断裂链条
    BrokenChain,
    /// 多头分支（同一版本被多个新版本取代）
    MultiHead,
}

/// 版本冲突
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionConflict {
    /// 冲突类型
    pub conflict_type: VersionConflictType,
    /// 涉及的实体ID列表
    pub involved_ids: Vec<String>,
    /// 冲突描述
    pub description: String,
    /// 建议的解决方案
    pub suggested_resolution: Option<String>,
}

/// 版本链管理器
pub struct SupersedesChainManager {
    /// 版本条目存储（entity_id -> entry）
    entries: HashMap<String, VersionChainEntry>,
    /// 链ID索引（chain_id -> entity_ids）
    chain_index: HashMap<String, Vec<String>>,
    /// 冲突记录
    conflicts: Vec<VersionConflict>,
}

impl SupersedesChainManager {
    /// 创建新的版本链管理器
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            chain_index: HashMap::new(),
            conflicts: Vec::new(),
        }
    }

    /// 注册新版本（创建版本链的根）
    pub fn register_root_version(
        &mut self,
        entity_id: impl Into<String>,
        entity_type: VersionedEntityType,
        created_at_ms: i64,
    ) -> VersionChainEntry {
        let entity_id = entity_id.into();
        let chain_id = Uuid::new_v4().to_string();

        let entry = VersionChainEntry {
            entity_id: entity_id.clone(),
            entity_type,
            supersedes: None,
            superseded_by: None,
            version: 1,
            created_at_ms,
            chain_id: chain_id.clone(),
            is_current: true,
            description: None,
        };

        self.entries.insert(entity_id.clone(), entry.clone());
        self.chain_index
            .entry(chain_id)
            .or_default()
            .push(entity_id);

        entry
    }

    /// 创建新版本（取代现有版本）
    ///
    /// # Arguments
    /// * `new_entity_id` - 新版本的实体ID
    /// * `supersedes_id` - 被取代的版本ID
    /// * `created_at_ms` - 创建时间戳
    /// * `description` - 版本说明
    pub fn create_new_version(
        &mut self,
        new_entity_id: impl Into<String>,
        supersedes_id: impl Into<String>,
        created_at_ms: i64,
        description: Option<String>,
    ) -> Result<VersionChainEntry, VersionConflict> {
        let new_entity_id = new_entity_id.into();
        let supersedes_id = supersedes_id.into();

        // 检查被取代的版本是否存在
        let old_entry = self.entries.get(&supersedes_id).cloned().ok_or_else(|| {
            VersionConflict {
                conflict_type: VersionConflictType::OrphanVersion,
                involved_ids: vec![new_entity_id.clone(), supersedes_id.clone()],
                description: format!("Superseded version {} not found", supersedes_id),
                suggested_resolution: Some(
                    "Register the superseded version first or create as root version".into(),
                ),
            }
        })?;

        // 检查是否已被其他版本取代
        if let Some(existing_successor) = &old_entry.superseded_by {
            let conflict = VersionConflict {
                conflict_type: VersionConflictType::DuplicateSupersession,
                involved_ids: vec![
                    new_entity_id.clone(),
                    supersedes_id.clone(),
                    existing_successor.clone(),
                ],
                description: format!(
                    "Version {} is already superseded by {}",
                    supersedes_id, existing_successor
                ),
                suggested_resolution: Some(
                    "Supersede the existing successor instead or create branch".into(),
                ),
            };
            self.conflicts.push(conflict.clone());
            return Err(conflict);
        }

        // 检查循环引用
        if self.would_create_cycle(&new_entity_id, &supersedes_id) {
            let conflict = VersionConflict {
                conflict_type: VersionConflictType::CyclicReference,
                involved_ids: vec![new_entity_id.clone(), supersedes_id.clone()],
                description: format!(
                    "Creating version {} superseding {} would create a cycle",
                    new_entity_id, supersedes_id
                ),
                suggested_resolution: Some("Review the version chain for errors".into()),
            };
            self.conflicts.push(conflict.clone());
            return Err(conflict);
        }

        // 创建新版本条目
        let new_version = old_entry.version + 1;
        let chain_id = old_entry.chain_id.clone();

        let new_entry = VersionChainEntry {
            entity_id: new_entity_id.clone(),
            entity_type: old_entry.entity_type,
            supersedes: Some(supersedes_id.clone()),
            superseded_by: None,
            version: new_version,
            created_at_ms,
            chain_id: chain_id.clone(),
            is_current: true,
            description,
        };

        // 更新旧版本
        if let Some(old) = self.entries.get_mut(&supersedes_id) {
            old.superseded_by = Some(new_entity_id.clone());
            old.is_current = false;
        }

        // 存储新版本
        self.entries.insert(new_entity_id.clone(), new_entry.clone());
        self.chain_index
            .entry(chain_id)
            .or_default()
            .push(new_entity_id);

        Ok(new_entry)
    }

    /// 检查是否会创建循环引用
    fn would_create_cycle(&self, new_id: &str, supersedes_id: &str) -> bool {
        // 检查 supersedes_id 的祖先链中是否包含 new_id
        let mut current = Some(supersedes_id.to_string());
        while let Some(id) = current {
            if id == new_id {
                return true;
            }
            current = self
                .entries
                .get(&id)
                .and_then(|e| e.supersedes.clone());
        }
        false
    }

    /// 获取版本链摘要
    pub fn get_chain_summary(&self, entity_id: &str) -> Option<VersionChainSummary> {
        let entry = self.entries.get(entity_id)?;
        let chain_id = &entry.chain_id;
        let chain_entries = self.chain_index.get(chain_id)?;

        // 找到根版本
        let root_id = self.find_root_version(entity_id)?;

        // 找到当前版本
        let current_id = self.find_current_version(&root_id)?;

        // 按版本号排序
        let mut version_ids: Vec<_> = chain_entries
            .iter()
            .filter_map(|id| self.entries.get(id))
            .collect();
        version_ids.sort_by_key(|e| e.version);
        let version_ids: Vec<_> = version_ids.iter().map(|e| e.entity_id.clone()).collect();

        // 获取时间范围
        let created_at_ms = version_ids
            .first()
            .and_then(|id| self.entries.get(id))
            .map(|e| e.created_at_ms)
            .unwrap_or(0);
        let updated_at_ms = version_ids
            .last()
            .and_then(|id| self.entries.get(id))
            .map(|e| e.created_at_ms)
            .unwrap_or(0);

        Some(VersionChainSummary {
            chain_id: chain_id.clone(),
            entity_type: entry.entity_type,
            chain_length: version_ids.len() as u32,
            root_id,
            current_id,
            version_ids,
            created_at_ms,
            updated_at_ms,
        })
    }

    /// 查找根版本
    pub fn find_root_version(&self, entity_id: &str) -> Option<String> {
        let mut current_id = entity_id.to_string();
        loop {
            let entry = self.entries.get(&current_id)?;
            match &entry.supersedes {
                Some(prev_id) => current_id = prev_id.clone(),
                None => return Some(current_id),
            }
        }
    }

    /// 查找当前（最新）版本
    pub fn find_current_version(&self, entity_id: &str) -> Option<String> {
        let mut current_id = entity_id.to_string();
        loop {
            let entry = self.entries.get(&current_id)?;
            match &entry.superseded_by {
                Some(next_id) => current_id = next_id.clone(),
                None => return Some(current_id),
            }
        }
    }

    /// 获取版本历史（从根到当前）
    pub fn get_version_history(&self, entity_id: &str) -> Vec<VersionChainEntry> {
        let root_id = match self.find_root_version(entity_id) {
            Some(id) => id,
            None => return vec![],
        };

        let mut history = Vec::new();
        let mut current_id = Some(root_id);

        while let Some(id) = current_id {
            if let Some(entry) = self.entries.get(&id).cloned() {
                current_id = entry.superseded_by.clone();
                history.push(entry);
            } else {
                break;
            }
        }

        history
    }

    /// 获取特定版本号的实体
    pub fn get_version(&self, entity_id: &str, version: u32) -> Option<&VersionChainEntry> {
        let root_id = self.find_root_version(entity_id)?;
        let entry = self.entries.get(&root_id)?;
        let chain_entries = self.chain_index.get(&entry.chain_id)?;

        for id in chain_entries {
            if let Some(e) = self.entries.get(id) {
                if e.version == version {
                    return Some(e);
                }
            }
        }
        None
    }

    /// 检测版本链完整性问题
    pub fn detect_conflicts(&mut self) -> Vec<VersionConflict> {
        let mut new_conflicts = Vec::new();

        // 检测断裂链条
        for (entity_id, entry) in &self.entries {
            // 检查前驱是否存在
            if let Some(supersedes_id) = &entry.supersedes {
                if !self.entries.contains_key(supersedes_id) {
                    new_conflicts.push(VersionConflict {
                        conflict_type: VersionConflictType::BrokenChain,
                        involved_ids: vec![entity_id.clone(), supersedes_id.clone()],
                        description: format!(
                            "Version {} references missing superseded version {}",
                            entity_id, supersedes_id
                        ),
                        suggested_resolution: Some("Restore the missing version or relink".into()),
                    });
                }
            }

            // 检查后继是否存在
            if let Some(superseded_by_id) = &entry.superseded_by {
                if !self.entries.contains_key(superseded_by_id) {
                    new_conflicts.push(VersionConflict {
                        conflict_type: VersionConflictType::BrokenChain,
                        involved_ids: vec![entity_id.clone(), superseded_by_id.clone()],
                        description: format!(
                            "Version {} references missing successor version {}",
                            entity_id, superseded_by_id
                        ),
                        suggested_resolution: Some("Restore the missing version or unlink".into()),
                    });
                }
            }
        }

        // 检测多头分支
        let mut supersedes_count: HashMap<String, Vec<String>> = HashMap::new();
        for (entity_id, entry) in &self.entries {
            if let Some(supersedes_id) = &entry.supersedes {
                supersedes_count
                    .entry(supersedes_id.clone())
                    .or_default()
                    .push(entity_id.clone());
            }
        }

        for (supersedes_id, successors) in supersedes_count {
            if successors.len() > 1 {
                let mut involved = successors.clone();
                involved.push(supersedes_id.clone());
                new_conflicts.push(VersionConflict {
                    conflict_type: VersionConflictType::MultiHead,
                    involved_ids: involved,
                    description: format!(
                        "Version {} has multiple successors: {:?}",
                        supersedes_id, successors
                    ),
                    suggested_resolution: Some(
                        "Merge conflicting versions or mark one as canonical".into(),
                    ),
                });
            }
        }

        self.conflicts.extend(new_conflicts.clone());
        new_conflicts
    }

    /// 获取所有冲突
    pub fn get_conflicts(&self) -> &[VersionConflict] {
        &self.conflicts
    }

    /// 清除冲突记录
    pub fn clear_conflicts(&mut self) {
        self.conflicts.clear();
    }

    /// 获取实体的版本条目
    pub fn get_entry(&self, entity_id: &str) -> Option<&VersionChainEntry> {
        self.entries.get(entity_id)
    }

    /// 获取所有链的统计信息
    pub fn get_statistics(&self) -> VersionChainStatistics {
        let mut stats = VersionChainStatistics::default();

        stats.total_entries = self.entries.len();
        stats.total_chains = self.chain_index.len();
        stats.total_conflicts = self.conflicts.len();

        for entries in self.chain_index.values() {
            let len = entries.len();
            if len > stats.max_chain_length {
                stats.max_chain_length = len;
            }
            stats.total_chain_length += len;
        }

        if stats.total_chains > 0 {
            stats.avg_chain_length = stats.total_chain_length as f32 / stats.total_chains as f32;
        }

        // 统计各类型数量
        for entry in self.entries.values() {
            match entry.entity_type {
                VersionedEntityType::DialogueEvent => stats.dialogue_event_count += 1,
                VersionedEntityType::Session => stats.session_count += 1,
                VersionedEntityType::Message => stats.message_count += 1,
                VersionedEntityType::Artifact => stats.artifact_count += 1,
            }
        }

        stats
    }
}

impl Default for SupersedesChainManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 版本链统计信息
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VersionChainStatistics {
    /// 总条目数
    pub total_entries: usize,
    /// 总链数
    pub total_chains: usize,
    /// 总冲突数
    pub total_conflicts: usize,
    /// 最长链长度
    pub max_chain_length: usize,
    /// 总链长度（用于计算平均值）
    pub total_chain_length: usize,
    /// 平均链长度
    pub avg_chain_length: f32,
    /// 对话事件数量
    pub dialogue_event_count: usize,
    /// 会话数量
    pub session_count: usize,
    /// 消息数量
    pub message_count: usize,
    /// 工件数量
    pub artifact_count: usize,
}

/// 用于构建版本链的 SurrealQL 查询示例
pub struct SupersedesChainQueries;

impl SupersedesChainQueries {
    /// 查询实体的完整版本链
    pub fn get_full_chain_query() -> &'static str {
        r#"
        -- 获取完整版本链
        -- 输入: $entity_id
        LET $root = (
            SELECT VALUE id FROM ONLY (
                TRAVERSE supersedes FROM type::thing($entity_id)
                WHERE supersedes != NONE
            )
        ) OR $entity_id;

        SELECT * FROM (
            TRAVERSE superseded_by FROM type::thing($root)
        ) ORDER BY version ASC
        "#
    }

    /// 查询最新版本
    pub fn get_current_version_query() -> &'static str {
        r#"
        -- 获取最新版本
        -- 输入: $entity_id
        SELECT * FROM ONLY (
            TRAVERSE superseded_by FROM type::thing($entity_id)
            WHERE superseded_by = NONE
        )
        "#
    }

    /// 查询特定版本
    pub fn get_specific_version_query() -> &'static str {
        r#"
        -- 获取特定版本
        -- 输入: $entity_id, $version
        LET $root = (
            SELECT VALUE id FROM ONLY (
                TRAVERSE supersedes FROM type::thing($entity_id)
                WHERE supersedes = NONE
            )
        ) OR $entity_id;

        SELECT * FROM (
            TRAVERSE superseded_by FROM type::thing($root)
            WHERE version = $version
        )
        "#
    }

    /// 创建新版本（更新关系）
    pub fn create_new_version_query() -> &'static str {
        r#"
        -- 创建新版本
        -- 输入: $new_id, $supersedes_id, $description
        BEGIN TRANSACTION;

        -- 更新旧版本
        UPDATE type::thing($supersedes_id) SET
            superseded_by = $new_id,
            is_current = false;

        -- 获取旧版本信息
        LET $old = (SELECT * FROM type::thing($supersedes_id));

        -- 创建新版本
        UPDATE type::thing($new_id) SET
            supersedes = $supersedes_id,
            superseded_by = NONE,
            version = $old.version + 1,
            chain_id = $old.chain_id,
            is_current = true,
            description = $description;

        COMMIT TRANSACTION;
        "#
    }

    /// 检测冲突的查询
    pub fn detect_conflicts_query() -> &'static str {
        r#"
        -- 检测多头分支冲突
        SELECT
            supersedes,
            array::group(id) as successors,
            count() as successor_count
        FROM versioned_entity
        WHERE supersedes != NONE
        GROUP BY supersedes
        HAVING count() > 1
        "#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_root_version() {
        let mut manager = SupersedesChainManager::new();
        let entry =
            manager.register_root_version("event-1", VersionedEntityType::DialogueEvent, 1000);

        assert_eq!(entry.entity_id, "event-1");
        assert_eq!(entry.version, 1);
        assert!(entry.supersedes.is_none());
        assert!(entry.superseded_by.is_none());
        assert!(entry.is_current);
    }

    #[test]
    fn test_create_new_version() {
        let mut manager = SupersedesChainManager::new();
        manager.register_root_version("event-1", VersionedEntityType::DialogueEvent, 1000);

        let result =
            manager.create_new_version("event-2", "event-1", 2000, Some("Updated".into()));
        assert!(result.is_ok());

        let new_entry = result.unwrap();
        assert_eq!(new_entry.entity_id, "event-2");
        assert_eq!(new_entry.version, 2);
        assert_eq!(new_entry.supersedes, Some("event-1".into()));
        assert!(new_entry.is_current);

        // 检查旧版本已更新
        let old_entry = manager.get_entry("event-1").unwrap();
        assert_eq!(old_entry.superseded_by, Some("event-2".into()));
        assert!(!old_entry.is_current);
    }

    #[test]
    fn test_version_chain() {
        let mut manager = SupersedesChainManager::new();
        manager.register_root_version("v1", VersionedEntityType::Session, 1000);
        manager
            .create_new_version("v2", "v1", 2000, None)
            .unwrap();
        manager
            .create_new_version("v3", "v2", 3000, None)
            .unwrap();

        // 测试查找根版本
        assert_eq!(manager.find_root_version("v3"), Some("v1".into()));

        // 测试查找当前版本
        assert_eq!(manager.find_current_version("v1"), Some("v3".into()));

        // 测试版本历史
        let history = manager.get_version_history("v2");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].entity_id, "v1");
        assert_eq!(history[1].entity_id, "v2");
        assert_eq!(history[2].entity_id, "v3");
    }

    #[test]
    fn test_duplicate_supersession_conflict() {
        let mut manager = SupersedesChainManager::new();
        manager.register_root_version("v1", VersionedEntityType::Message, 1000);
        manager.create_new_version("v2", "v1", 2000, None).unwrap();

        // 尝试再次取代 v1 应该失败
        let result = manager.create_new_version("v3", "v1", 3000, None);
        assert!(result.is_err());

        let conflict = result.unwrap_err();
        assert_eq!(
            conflict.conflict_type,
            VersionConflictType::DuplicateSupersession
        );
    }

    #[test]
    fn test_chain_summary() {
        let mut manager = SupersedesChainManager::new();
        manager.register_root_version("v1", VersionedEntityType::Artifact, 1000);
        manager.create_new_version("v2", "v1", 2000, None).unwrap();
        manager.create_new_version("v3", "v2", 3000, None).unwrap();

        let summary = manager.get_chain_summary("v2").unwrap();
        assert_eq!(summary.chain_length, 3);
        assert_eq!(summary.root_id, "v1");
        assert_eq!(summary.current_id, "v3");
        assert_eq!(summary.version_ids, vec!["v1", "v2", "v3"]);
    }

    #[test]
    fn test_statistics() {
        let mut manager = SupersedesChainManager::new();
        manager.register_root_version("event-1", VersionedEntityType::DialogueEvent, 1000);
        manager.register_root_version("session-1", VersionedEntityType::Session, 1000);
        manager
            .create_new_version("event-2", "event-1", 2000, None)
            .unwrap();

        let stats = manager.get_statistics();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.total_chains, 2);
        assert_eq!(stats.dialogue_event_count, 2);
        assert_eq!(stats.session_count, 1);
    }
}

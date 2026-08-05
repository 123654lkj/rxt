/// 本地索引追踪 — 记录哪些文件已索引、MD5 是否变化
///
/// 存储位置: <project>/.rxt-cache/seek-index.json
/// 和 map.rs 的缓存模式一致（JSON 文件，不引入 SQLite）

use std::path::Path;
use serde::{Deserialize, Serialize};
use super::provider::CodeChunk;

/// 索引状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeekIndex {
    /// 项目名
    pub project: String,
    /// 后端名称
    pub provider: String,
    /// 上次索引时间
    pub last_indexed: Option<String>,
    /// 文件追踪: path → 索引信息
    pub files: std::collections::HashMap<String, FileRecord>,
}

/// 单个文件的索引记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    /// 文件内容 MD5
    pub md5: String,
    /// 该文件产生的 chunk 数
    pub chunks: usize,
    /// 索引时间
    pub indexed_at: String,
}

impl SeekIndex {
    /// 从项目目录加载索引
    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        let path = index_path(project_root);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let idx: SeekIndex = serde_json::from_str(&content)?;
            Ok(idx)
        } else {
            Ok(Self {
                project: String::new(),
                provider: String::new(),
                last_indexed: None,
                files: std::collections::HashMap::new(),
            })
        }
    }

    /// 保存索引
    pub fn save(&self, project_root: &Path) -> anyhow::Result<()> {
        let path = index_path(project_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// 检查文件是否需要重新索引
    pub fn needs_reindex(&self, file_path: &str, current_md5: &str) -> bool {
        match self.files.get(file_path) {
            Some(record) => record.md5 != current_md5,
            None => true, // 新文件
        }
    }

    /// 更新文件记录
    pub fn update_file(&mut self, file_path: &str, md5: &str, chunks: usize) {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        self.files.insert(file_path.to_string(), FileRecord {
            md5: md5.to_string(),
            chunks,
            indexed_at: now.clone(),
        });
        self.last_indexed = Some(now);
    }

    /// 删除不再存在的文件记录
    pub fn remove_stale(&mut self, current_files: &[String]) {
        let current_set: std::collections::HashSet<&str> =
            current_files.iter().map(|s| s.as_str()).collect();
        self.files.retain(|k, _| current_set.contains(k.as_str()));
    }

    /// 统计
    pub fn total_chunks(&self) -> usize {
        self.files.values().map(|r| r.chunks).sum()
    }

    pub fn total_files(&self) -> usize {
        self.files.len()
    }
}

fn index_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".rxt-cache").join("seek-index.json")
}

/// 过滤出需要重新索引的 chunks (返回拥有的 clone)
pub fn filter_changed_chunks(
    chunks: &[CodeChunk],
    index: &SeekIndex,
) -> Vec<CodeChunk> {
    let mut changed_files = std::collections::HashSet::new();

    for chunk in chunks {
        if index.needs_reindex(&chunk.file, &chunk.md5) {
            changed_files.insert(chunk.file.clone());
        }
    }

    chunks.iter()
        .filter(|c| changed_files.contains(&c.file))
        .cloned()
        .collect()
}

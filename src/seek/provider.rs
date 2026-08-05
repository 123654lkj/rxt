/// 搜索提供者接口
///
/// 所有后端（星枢、Ollama、本地模型...）都实现这个 trait。
/// seek 命令只依赖 trait，不关心具体后端。

use serde::{Deserialize, Serialize};

// ============================== 数据结构 ==============================

/// 代码块 — 索引和搜索的基本单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// 相对路径 (如 "src/main.rs")
    pub file: String,
    /// 起始行 (1-indexed)
    pub line: usize,
    /// 结束行
    pub end_line: usize,
    /// 符号名 (如 "fn run", "struct Cli")
    pub name: String,
    /// 符号类型: fn / struct / enum / trait / impl / method
    pub kind: String,
    /// 语言: rust / python / go / javascript / typescript / ...
    pub language: String,
    /// 完整代码内容 (签名 + 函数体)
    pub content: String,
    /// 内容 MD5 (增量更新用)
    pub md5: String,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk: CodeChunk,
    /// 相关度分数 (0.0~1.0, 越高越好)
    pub score: f64,
    /// 来自哪个后端
    pub provider: String,
}

/// 索引统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub provider: String,
    pub project: String,
    pub total_chunks: usize,
    pub total_files: usize,
    pub last_indexed: Option<String>,
}

/// 搜索后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeekConfig {
    /// 后端名称: "nebula" / "ollama" / "local"
    #[serde(default = "default_provider")]
    pub provider: String,
    /// 星枢地址
    #[serde(default = "default_nebula_url")]
    pub nebula_url: String,
    /// 项目名 (默认用目录名)
    #[serde(default)]
    pub project: Option<String>,
}

fn default_provider() -> String { "nebula".into() }
fn default_nebula_url() -> String { "http://192.168.31.252:26670".into() }

impl Default for SeekConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            nebula_url: default_nebula_url(),
            project: None,
        }
    }
}

impl SeekConfig {
    /// 从 ~/.rxt/seek.toml 加载，不存在则用默认值
    pub fn load() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("无法获取 home 目录"))?;
        let path = home.join(".rxt").join("seek.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let cfg: SeekConfig = toml::from_str(&content)?;
            Ok(cfg)
        } else {
            Ok(Self::default())
        }
    }
}

// ============================== Trait ==============================

/// 搜索提供者 — 接口化的核心
///
/// 所有后端实现这个 trait:
/// - NebulaProvider: 调星枢 API (当前)
/// - OllamaProvider: 调 Ollama 本地模型 (未来)
/// - LocalProvider: 本地 embedding + 向量检索 (未来)
pub trait SearchProvider {
    /// 后端名称
    fn name(&self) -> &str;

    /// 索引一批代码块
    /// 返回成功索引的 chunk 数
    fn index(&self, chunks: &[CodeChunk], project: &str) -> anyhow::Result<usize>;

    /// 语义搜索
    fn search(
        &self,
        query: &str,
        top_k: usize,
        project: &str,
        language: Option<&str>,
    ) -> anyhow::Result<Vec<SearchHit>>;

    /// 清除项目的所有索引
    fn clear(&self, project: &str) -> anyhow::Result<()>;

    /// 索引统计
    fn stats(&self, project: &str) -> anyhow::Result<IndexStats>;
}

/// 根据配置创建对应的后端
pub fn create_provider(cfg: &SeekConfig) -> anyhow::Result<Box<dyn SearchProvider>> {
    match cfg.provider.as_str() {
        "nebula" => {
            let p = crate::seek::nebula::NebulaProvider::new(&cfg.nebula_url)?;
            Ok(Box::new(p))
        }
        other => anyhow::bail!("未知的搜索后端: {} (支持: nebula)", other),
    }
}

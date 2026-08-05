/// seek — 代码语义搜索
///
/// 用法:
///   rxt seek "处理SSH连接的函数"     # 语义搜索
///   rxt seek --index                 # 构建/更新索引
///   rxt seek --stats                 # 查看索引统计
///   rxt seek --clear                 # 清除索引
///   rxt seek "xxx" -k 10 --type rust # 限定条数和语言
///   rxt seek "xxx" --json            # JSON 输出

pub mod provider;
pub mod chunk;
pub mod store;
pub mod nebula;

use std::path::Path;
use crate::seek::provider::{SeekConfig, create_provider};

pub fn run(
    query: Option<&str>,
    path: Option<&Path>,
    index: bool,
    stats: bool,
    clear: bool,
    top_k: usize,
    language: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let root = path.unwrap_or_else(|| Path::new("."));
    let root = std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf());

    // 加载配置
    let cfg = SeekConfig::load().unwrap_or_default();

    // 项目名: 配置指定 > 目录名
    let project = cfg.project.clone()
        .unwrap_or_else(|| {
            root.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    // 创建后端
    let provider = create_provider(&cfg)?;

    // 分发
    if index {
        return do_index(&root, &project, provider.as_ref(), json);
    }

    if stats {
        return do_stats(&root, &project, provider.as_ref(), json);
    }

    if clear {
        return provider.clear(&project);
    }

    // 默认: 搜索
    let q = query.ok_or_else(|| {
        anyhow::anyhow!("请提供搜索关键词，或用 --index 构建索引")
    })?;

    do_search(q, &project, provider.as_ref(), top_k, language, json)
}

/// 构建/更新索引
fn do_index(
    root: &Path,
    project: &str,
    provider: &dyn crate::seek::provider::SearchProvider,
    json: bool,
) -> anyhow::Result<()> {
    let mut index = store::SeekIndex::load(root)?;
    index.project = project.to_string();
    index.provider = provider.name().to_string();

    eprintln!("  扫描项目: {}", root.display());
    let all_chunks = chunk::scan_project(root)?;

    if all_chunks.is_empty() {
        eprintln!("  没有找到可索引的代码块");
        return Ok(());
    }

    // 过滤出需要重新索引的
    let changed = store::filter_changed_chunks(&all_chunks, &index);
    let unchanged = all_chunks.len() - changed.len();

    if !json {
        eprintln!("  总代码块: {} | 变更: {} | 未变: {}",
            all_chunks.len(), changed.len(), unchanged);
    }

    if changed.is_empty() {
        if !json {
            eprintln!("  索引已是最新，无需更新");
        }
        return Ok(());
    }

    eprintln!("  正在索引 {} 个变更代码块...", changed.len());
    let indexed = provider.index(&changed, project)?;

    // 更新本地追踪
    for c in &changed {
        let file_chunks = all_chunks.iter()
            .filter(|cc| cc.file == c.file)
            .count();
        index.update_file(&c.file, &c.md5, file_chunks);
    }
    let current_files: Vec<String> = all_chunks.iter()
        .map(|c| c.file.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    index.remove_stale(&current_files);
    index.save(root)?;

    if json {
        let result = serde_json::json!({
            "status": "ok",
            "total_chunks": all_chunks.len(),
            "indexed": indexed,
            "unchanged": unchanged,
            "project": project,
            "provider": provider.name(),
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        eprintln!("  索引完成: {} 个代码块 → {}", indexed, provider.name());
    }

    Ok(())
}

/// 语义搜索
fn do_search(
    query: &str,
    project: &str,
    provider: &dyn crate::seek::provider::SearchProvider,
    top_k: usize,
    language: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let hits = provider.search(query, top_k, project, language)?;

    if hits.is_empty() {
        if json {
            println!("{{\"results\":[],\"count\":0}}");
        } else {
            eprintln!("  没有找到相关结果");
        }
        return Ok(());
    }

    if json {
        let result = serde_json::json!({
            "results": hits,
            "count": hits.len(),
            "query": query,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        for (i, hit) in hits.iter().enumerate() {
            println!("{}. {} [{}] (score: {:.3})",
                i + 1, hit.chunk.name, hit.chunk.file, hit.score);
            // 显示内容摘要（前 3 行）
            for line in hit.chunk.content.lines().take(3) {
                println!("   {}", truncate(line, 80));
            }
            println!();
        }
    }

    Ok(())
}

/// 索引统计
fn do_stats(
    root: &Path,
    project: &str,
    provider: &dyn crate::seek::provider::SearchProvider,
    json: bool,
) -> anyhow::Result<()> {
    let index = store::SeekIndex::load(root)?;

    if json {
        let result = serde_json::json!({
            "provider": provider.name(),
            "project": project,
            "local_files": index.total_files(),
            "local_chunks": index.total_chunks(),
            "last_indexed": index.last_indexed,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("  后端: {}", provider.name());
        println!("  项目: {}", project);
        println!("  已索引文件: {}", index.total_files());
        println!("  已索引代码块: {}", index.total_chunks());
        if let Some(ref t) = index.last_indexed {
            println!("  上次索引: {}", t);
        }
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len.saturating_sub(3)]
    }
}

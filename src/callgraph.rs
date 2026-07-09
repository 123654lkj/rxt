//! callgraph — 全库调用图引擎 (v0.8.0)
//!
//! 构建"符号节点 + 调用边"的调用图, 供 dead/trace/impact 命令共享.
//! 纯文本启发式 (无 tree-sitter), 复用 langparse 符号提取 + refs 调用识别.
//!
//! 灵感: codeseek 调用图 + codebase-memory-mcp 死代码/影响分析 + loop-engineering impact.

use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet, BTreeMap};

/// 符号节点 — 调用图中的一个函数/方法
#[derive(Debug, Clone)]
pub struct SymNode {
    pub name: String,
    pub kind: String,       // "fn" / "def" / "function" / "func" / "method"
    pub file: String,       // 相对路径
    pub line: usize,        // 行号 (1-indexed)
    pub is_entry: bool,     // 是否入口 (main/pub/export)
    pub is_test: bool,      // 是否测试函数
}

/// 调用图
pub struct CallGraph {
    /// 所有符号节点
    nodes: Vec<SymNode>,
    /// 符号名 → 节点索引列表 (同名可能有多个)
    name_index: HashMap<String, Vec<usize>>,
    /// 正向边: caller_idx → [(callee_name, line), ...]
    forward_edges: Vec<Vec<(String, usize)>>,
    /// 反向边: callee_name → [caller_idx, ...] (谁调用了我)
    backward_edges: HashMap<String, Vec<usize>>,
    /// 入口节点索引
    entry_indices: Vec<usize>,
}

const CALLABLE_KINDS: &[&str] = &["fn", "def", "function", "func", "method"];

impl CallGraph {
    /// 构建全库调用图
    pub fn build(root: &Path) -> anyhow::Result<Self> {
        let exts = crate::langparse::Lang::all_known_exts();
        let ext_refs: Vec<&str> = exts.iter().copied().collect();
        let files = crate::common::walk_clean(root, Some(&ext_refs), None);

        let mut nodes: Vec<SymNode> = Vec::new();
        let mut forward_edges: Vec<Vec<(String, usize)>> = Vec::new();
        let mut file_symbol_map: HashMap<String, Vec<usize>> = HashMap::new(); // file → node indices

        // 第一遍: 收集所有符号节点
        for f in &files {
            let content = match std::fs::read_to_string(f) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lang = match crate::langparse::detect_lang(f) {
                Some(l) => l,
                None => continue,
            };
            let items = crate::langparse::parse(&content, lang);
            let rel = rel_path(root, f);
            let file_is_test = is_test_file(&rel, &content);

            for item in items {
                if !CALLABLE_KINDS.contains(&item.kind.as_str()) {
                    continue; // 只关心可调用的符号
                }
                let is_entry = is_entry_point(&item, &rel);
                let is_test = file_is_test || item.name.starts_with("test_") || item.name.starts_with("Test");
                let idx = nodes.len();
                nodes.push(SymNode {
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                    file: rel.clone(),
                    line: item.line,
                    is_entry,
                    is_test,
                });
                forward_edges.push(Vec::new());
                file_symbol_map.entry(rel.clone()).or_default().push(idx);
            }
        }

        // 构建 name_index
        let mut name_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            name_index.entry(node.name.clone()).or_default().push(idx);
        }

        // 第二遍: 提取调用边 (复用 refs::extract_calls_from_body)
        for f in &files {
            let content = match std::fs::read_to_string(f) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lang = match crate::langparse::detect_lang(f) {
                Some(l) => l,
                None => continue,
            };
            let items = crate::langparse::parse(&content, lang);
            let rel = rel_path(root, f);
            let lines: Vec<&str> = content.lines().collect();

            for item in &items {
                if !CALLABLE_KINDS.contains(&item.kind.as_str()) {
                    continue;
                }
                // 找这个符号在 nodes 里的 index
                let node_idx = match find_symbol_index(&nodes, &name_index, &item.name, &rel, item.line) {
                    Some(i) => i,
                    None => continue,
                };
                // 算函数体范围
                let body_len = crate::digest::count_body(&lines, item.line, &item.kind);
                let body_start = item.line.saturating_sub(1);
                let body_end = (item.line + body_len).min(lines.len());

                // 提取调用
                let calls = crate::refs::extract_calls_from_body(&lines, body_start, body_end);
                forward_edges[node_idx] = calls;
            }
        }

        // 构建反向边
        let mut backward_edges: HashMap<String, Vec<usize>> = HashMap::new();
        for (caller_idx, calls) in forward_edges.iter().enumerate() {
            for (callee_name, _) in calls {
                backward_edges.entry(callee_name.clone()).or_default().push(caller_idx);
            }
        }

        // 入口索引
        let entry_indices: Vec<usize> = nodes.iter().enumerate()
            .filter(|(_, n)| n.is_entry)
            .map(|(i, _)| i)
            .collect();

        Ok(CallGraph {
            nodes,
            name_index,
            forward_edges,
            backward_edges,
            entry_indices,
        })
    }

    /// 死代码检测: 从入口做可达性分析, 返回不可达的非测试符号
    pub fn dead_code(&self) -> Vec<&SymNode> {
        // 从入口 + 测试入口出发, BFS 标记可达
        let mut reachable = HashSet::new();
        let mut queue: Vec<usize> = Vec::new();

        // 入口节点
        for &idx in &self.entry_indices {
            queue.push(idx);
        }
        // 也从测试函数出发 (测试调用的代码不算死代码)
        for (idx, node) in self.nodes.iter().enumerate() {
            if node.is_test {
                queue.push(idx);
            }
        }

        while let Some(idx) = queue.pop() {
            if !reachable.insert(idx) {
                continue; // 已访问
            }
            // 沿正向边传播
            for (callee_name, _) in &self.forward_edges[idx] {
                if let Some(callee_indices) = self.name_index.get(callee_name) {
                    for &ci in callee_indices {
                        if !reachable.contains(&ci) {
                            queue.push(ci);
                        }
                    }
                }
            }
        }

        // 不可达的非测试符号 = 死代码
        self.nodes.iter().enumerate()
            .filter(|(idx, node)| !reachable.contains(idx) && !node.is_test)
            .map(|(_, n)| n)
            .collect()
    }

    /// 多跳调用链追踪
    /// downward=true: 它调用了谁 (正向); false: 谁调用了它 (反向)
    /// 返回树状结构: (depth, node_idx) 的列表, 已去环
    pub fn trace(&self, symbol: &str, max_depth: usize, downward: bool) -> Vec<(usize, &SymNode)> {
        let start_indices = self.name_index.get(symbol).cloned().unwrap_or_default();
        if start_indices.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: Vec<(usize, usize)> = start_indices.iter().map(|&i| (i, 0)).collect();

        while let Some((idx, depth)) = queue.pop() {
            if depth > max_depth {
                continue;
            }
            if !visited.insert(idx) {
                continue;
            }
            result.push((depth, &self.nodes[idx]));

            if depth < max_depth {
                if downward {
                    // 正向: 它调用了谁
                    for (callee_name, _) in &self.forward_edges[idx] {
                        if let Some(callee_indices) = self.name_index.get(callee_name) {
                            for &ci in callee_indices {
                                if !visited.contains(&ci) {
                                    queue.push((ci, depth + 1));
                                }
                            }
                        }
                    }
                } else {
                    // 反向: 谁调用了它
                    let node_name = &self.nodes[idx].name;
                    if let Some(callers) = self.backward_edges.get(node_name) {
                        for &caller_idx in callers {
                            if !visited.contains(&caller_idx) {
                                queue.push((caller_idx, depth + 1));
                            }
                        }
                    }
                }
            }
        }
        result
    }

    /// 改动爆炸半径: 给定改动的文件, 算出受影响的调用者链
    /// 返回 (distance, node) 列表, distance=0 是直接改动的符号
    pub fn impact(&self, changed_files: &[String]) -> Vec<(usize, &SymNode)> {
        // 找出改动文件里的所有符号
        let mut seeds: Vec<usize> = Vec::new();
        for (idx, node) in self.nodes.iter().enumerate() {
            if changed_files.iter().any(|f| &node.file == f) {
                seeds.push(idx);
            }
        }

        if seeds.is_empty() {
            return Vec::new();
        }

        // 反向 BFS: 从改动的符号出发, 沿"谁调用了我"传播
        let mut result: BTreeMap<(usize, String), usize> = BTreeMap::new(); // (depth, file:name) → node_idx
        let mut visited = HashSet::new();
        let mut queue: Vec<(usize, usize)> = seeds.iter().map(|&i| (i, 0)).collect();

        while let Some((idx, dist)) = queue.pop() {
            if !visited.insert(idx) {
                continue;
            }
            let node = &self.nodes[idx];
            result.insert((dist, format!("{}:{}", node.file, node.name)), idx);

            // 反向传播: 谁调用了这个符号
            let node_name = &node.name;
            if let Some(callers) = self.backward_edges.get(node_name) {
                for &caller_idx in callers {
                    if !visited.contains(&caller_idx) {
                        queue.push((caller_idx, dist + 1));
                    }
                }
            }
        }

        result.into_iter().map(|((d, _), idx)| (d, &self.nodes[idx])).collect()
    }

    /// 统计信息
    pub fn stats(&self) -> (usize, usize, usize) {
        let total_edges: usize = self.forward_edges.iter().map(|e| e.len()).sum();
        (self.nodes.len(), total_edges, self.entry_indices.len())
    }

    /// 查找符号是否存在
    pub fn find_node(&self, symbol: &str) -> Option<&SymNode> {
        self.name_index.get(symbol)
            .and_then(|indices| indices.first())
            .map(|&i| &self.nodes[i])
    }
}

// ============================== 辅助函数 ==============================

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_else(|_| p.display().to_string())
}

/// 判断是否入口点: main 函数 / pub fn (Rust) / export (JS/TS)
fn is_entry_point(item: &crate::langparse::CodeItem, file: &str) -> bool {
    // main 函数
    if item.name == "main" {
        return true;
    }
    // Rust: signature 含 pub
    if item.signature.contains("pub ") {
        return true;
    }
    // JS/TS: signature 含 export
    if item.signature.contains("export ") {
        return true;
    }
    // Go: 大写开头的函数是导出的 (包外可见 = 入口)
    if file.ends_with(".go") && item.name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        return true;
    }
    // Python: __init__ / setup / handle_* 等常见入口模式不强判
    false
}

/// 判断是否测试文件
fn is_test_file(file: &str, content: &str) -> bool {
    if file.contains("test") || file.contains("_test.go") || file.ends_with("_test.rs") {
        return true;
    }
    // Go: 文件头有 testing import
    if file.ends_with(".go") && content.contains("\"testing\"") {
        return true;
    }
    false
}

/// 在 nodes 里按 name+file+line 精确找 index
fn find_symbol_index(
    nodes: &[SymNode],
    _name_index: &HashMap<String, Vec<usize>>,
    name: &str,
    file: &str,
    line: usize,
) -> Option<usize> {
    // 精确匹配 name + file + line (最可靠)
    nodes.iter().position(|n| n.name == name && n.file == file && n.line == line)
}

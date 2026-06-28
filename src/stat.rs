//! 文件元信息 — 含完整文件指纹

use std::path::Path;
use std::fs;

use crate::signature::FileSignature;

pub fn run(path: &Path, json_output: bool, remote: Option<&crate::remote::RemoteChannel>) -> anyhow::Result<()> {
    let (raw, meta_modified, is_dir) = if let Some(remote) = remote {
        // 远程模式
        let raw = remote.read_file(path)?;
        // 通过 SSH 获取文件信息
        let info = remote.exec(&format!("stat -c '%Y %F' '{}'", path.display()))?;
        let parts: Vec<&str> = info.trim().splitn(2, ' ').collect();
        let modified = parts.get(0).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let file_type = parts.get(1).map(|s| *s).unwrap_or("file");
        let is_dir = file_type.contains("directory");
        (raw, modified, is_dir)
    } else {
        // 本地模式
        let meta = fs::metadata(path)?;
        let raw = fs::read(path)?;
        let modified = meta.modified()?.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        let is_dir = meta.is_dir();
        (raw, modified, is_dir)
    };
    
    let sig = FileSignature::detect(&raw);
    let file_type = if is_dir { "directory" } else { "file" };
    let modified_time = chrono::DateTime::<chrono::Utc>::from_timestamp(meta_modified, 0)
        .unwrap_or_default();

    if json_output {
        let json = serde_json::json!({
            "path": path.display().to_string(),
            "size_bytes": raw.len(),
            "size_kb": raw.len() / 1024,
            "modified": modified_time.to_rfc3339(),
            "type": file_type,
            "lines": sig.lines,
            "encoding": sig.encoding.to_string(),
            "line_ending": sig.line_ending.to_string(),
            "bom": sig.has_bom,
            "indent": sig.indent.to_string()
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        let local_modified: chrono::DateTime<chrono::Local> = modified_time.into();
        println!("Path:        {}", path.display());
        println!("Size:        {} bytes ({} KB)", raw.len(), raw.len() / 1024);
        println!("Modified:    {}", local_modified.format("%Y-%m-%d %H:%M:%S"));
        println!("Type:        {}", file_type);
        println!("Lines:       {}", sig.lines);
        println!("Encoding:    {}", sig.encoding);
        println!("LineEnding:  {}", sig.line_ending);
        println!("BOM:         {}", sig.has_bom);
        println!("Indent:      {}", sig.indent);
    }

    Ok(())
}

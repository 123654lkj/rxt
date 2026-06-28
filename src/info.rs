//! rxt 自身信息 / 自检

pub fn run(json_output: bool) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let rxt_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let hosts_path = crate::hosts::HostsFile::config_path().ok();
    let hosts_info = if let Some(path) = &hosts_path {
        if path.exists() {
            match crate::hosts::HostsFile::load() {
                Ok(h) => {
                    let host_names: Vec<String> = h.hosts.keys().cloned().collect();
                    let group_names: Vec<String> = h.group.keys().cloned().collect();
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "exists": true,
                        "hosts": host_names,
                        "groups": group_names,
                        "host_count": host_names.len(),
                        "group_count": group_names.len(),
                    })
                }
                Err(e) => serde_json::json!({
                    "path": path.display().to_string(),
                    "exists": true,
                    "error": e.to_string(),
                }),
            }
        } else {
            serde_json::json!({
                "path": path.display().to_string(),
                "exists": false,
            })
        }
    } else {
        serde_json::json!({"error": "cannot determine home dir"})
    };

    let info = serde_json::json!({
        "version": version,
        "rxt_binary": rxt_path,
        "hosts": hosts_info,
        "features": {
            "remote": true,
            "regex": true,
            "parallel_grep": true,
            "format_preserving": true,
        },
    });

    if json_output {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("rxt version: {}", version);
        println!("binary:     {}", rxt_path);
        if let Some(p) = hosts_path {
            println!("hosts file: {}", p.display());
            if p.exists() {
                println!("  (exists)");
            } else {
                println!("  (not found)");
            }
        }
    }
    Ok(())
}

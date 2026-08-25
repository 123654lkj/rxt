//! rxt 自身信息 / 自检

pub fn run(json_output: bool) -> anyhow::Result<()> {
    crate::common::setup_utf8_console();
    let version = env!("CARGO_PKG_VERSION");
    let rxt_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let hosts_path = crate::hosts::HostsFile::config_path().ok();
    let env_path = crate::hosts::HostsFile::env_path().ok();
    let mut insecure_hosts: Vec<String> = Vec::new();
    let mut missing_env: Vec<String> = Vec::new();
    let hosts_info = if let Some(path) = &hosts_path {
        if path.exists() {
            match crate::hosts::HostsFile::load() {
                Ok(h) => {
                    let host_names: Vec<String> = h.hosts.keys().cloned().collect();
                    let group_names: Vec<String> = h.group.keys().cloned().collect();
                    // 脱敏：只暴露 auth 方式，绝不 dump 密码
                    let mut auth_modes = serde_json::Map::new();
                    for (name, cfg) in &h.hosts {
                        let mode = crate::hosts::HostsFile::auth_summary(cfg);
                        auth_modes.insert(name.clone(), serde_json::json!(mode));
                        if mode.contains("INSECURE") || mode.contains("plaintext") {
                            insecure_hosts.push(name.clone());
                        }
                        // password_env 指向的变量是否可读（不打印值）
                        if let Some(ev) = &cfg.password_env {
                            if std::env::var(ev).map(|v| v.is_empty()).unwrap_or(true) {
                                missing_env.push(format!("{}:{}", name, ev));
                            }
                        }
                    }
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "exists": true,
                        "env_file": env_path.as_ref().map(|p| p.display().to_string()),
                        "env_file_exists": env_path.as_ref().map(|p| p.exists()).unwrap_or(false),
                        "hosts": host_names,
                        "groups": group_names,
                        "host_count": host_names.len(),
                        "group_count": group_names.len(),
                        "auth_modes": auth_modes,
                        "insecure_plaintext_hosts": insecure_hosts,
                        "missing_password_env": missing_env,
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
        "utf8": {
            "agent_capture_mode": crate::common::agent_capture_mode(),
            "stdout_is_tty": crate::common::stdout_is_tty(),
            "hint": "管道乱码: RXT_AGENT=1；禁 BOM: RXT_NO_BOM=1",
        },
        "nebula": {
            "url_env": std::env::var("RXT_NEBULA_URL").or_else(|_| std::env::var("NEBULA_URL")).ok(),
            "ssh_env": std::env::var("RXT_NEBULA_SSH").ok(),
            "default_url": "http://127.0.0.1:26670",
            "default_ssh": null,
        },
        "features": {
            "remote": true,
            "regex": true,
            "parallel_grep": true,
            "format_preserving": true,
            "mem_ssh_fallback": true,
            "password_env_first": true,
            "agent_utf8_bom": true,
        },
    });

    let text = if json_output {
        serde_json::to_string_pretty(&info)?
    } else {
        let mut lines = vec![
            format!("rxt version: {}", version),
            format!("binary:     {}", rxt_path),
        ];
        if let Some(p) = &hosts_path {
            lines.push(format!("hosts file: {}", p.display()));
            if p.exists() {
                lines.push("  (exists)".into());
            } else {
                lines.push("  (not found)".into());
            }
        }
        if let Some(p) = &env_path {
            lines.push(format!(
                "env file:   {} ({})",
                p.display(),
                if p.exists() { "exists" } else { "missing" }
            ));
        }
        if let Some(modes) = info["hosts"]["auth_modes"].as_object() {
            lines.push("auth modes (redacted, no secrets):".into());
            for (name, mode) in modes {
                lines.push(format!("  {}: {}", name, mode.as_str().unwrap_or("?")));
            }
        }
        if !insecure_hosts.is_empty() {
            lines.push(format!(
                "⚠ plaintext password hosts: {} — 请改 password_env",
                insecure_hosts.join(", ")
            ));
        }
        if !missing_env.is_empty() {
            lines.push(format!(
                "⚠ missing env values: {} — 检查 ~/.rxt/env",
                missing_env.join(", ")
            ));
        }
        lines.push(format!(
            "utf8 agent_capture={} tty={}",
            crate::common::agent_capture_mode(),
            crate::common::stdout_is_tty()
        ));
        lines.push("features: password_env_first, mem_ssh_fallback, agent_utf8_bom".into());
        lines.join("\n")
    };

    let mut stdout = std::io::stdout().lock();
    let _ = crate::common::maybe_write_bom(&mut stdout);
    use std::io::Write;
    let _ = writeln!(stdout, "{}", text);
    Ok(())
}

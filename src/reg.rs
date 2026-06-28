//! 注册表读写
//! 对标 PowerShell: Get/Set/Remove-ItemProperty, Get-ChildItem
//!
//! 策略: 包装 reg.exe (Windows 自带),避免 Win32 Reg* API 的复杂度。
//! 仅 Windows。
//!
//! 用法:
//!   rxt reg --get "HKLM\Software\Microsoft\Windows NT\CurrentVersion" --name ProductName
//!   rxt reg --list "HKLM\Software\Microsoft\Windows\CurrentVersion\Run"
//!   rxt reg --set "HKCU\Software\MyApp" --name Ver --value "1.0"
//!   rxt reg --delete "HKCU\Software\MyApp" --name Ver
//!   rxt reg --get "HKLM\..." --json

use std::process::Command;

pub fn run(get: Option<&str>, set: Option<&str>, delete: Option<&str>, value_name: Option<&str>, value: Option<&str>, list: Option<&str>, json: bool) -> anyhow::Result<()> {
    if cfg!(not(target_os = "windows")) {
        anyhow::bail!("reg 命令仅支持 Windows");
    }
    // 互斥操作
    if let Some(path) = list {
        return do_list(path, json);
    }
    if let Some(path) = get {
        return do_get(path, value_name, json);
    }
    if let Some(path) = set {
        let v = value.ok_or_else(|| anyhow::anyhow!("--set 需要 --value"))?;
        return do_set(path, value_name, v);
    }
    if let Some(path) = delete {
        return do_delete(path, value_name);
    }
    anyhow::bail!("需要指定 --get/--set/--delete/--list 之一")
}

fn do_get(path: &str, name: Option<&str>, json: bool) -> anyhow::Result<()> {
    let nm = name.unwrap_or(""); // 空 = 默认值
    let out = Command::new("reg").args(["query", path, "/v", nm]).output()
        .map_err(|e| anyhow::anyhow!("reg.exe 调用失败: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("读取失败: {}", err.trim());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // 解析 "    ProductName    REG_SZ    Windows 10 Pro"
    let mut val_type = String::new();
    let mut val = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.contains("REG_") {
            let parts: Vec<&str> = l.splitn(3, "REG_").collect();
            if parts.len() == 2 {
                // 找类型
                let after = format!("REG_{}", parts[1]);
                let tokens: Vec<&str> = after.split_whitespace().collect();
                if tokens.len() >= 2 {
                    val_type = tokens[0].to_string();
                    val = tokens[1..].join(" ");
                }
            }
        }
    }
    if json {
        let obj = serde_json::json!({
            "path": path, "name": nm, "type": val_type, "value": val,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else {
        println!("{}\\{}", path, if nm.is_empty() { "(默认)" } else { nm });
        println!("  类型: {}", val_type);
        println!("  值:   {}", val);
    }
    Ok(())
}

fn do_list(path: &str, json: bool) -> anyhow::Result<()> {
    let out = Command::new("reg").args(["query", path]).output()
        .map_err(|e| anyhow::anyhow!("reg.exe 调用失败: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("读取失败: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut values: Vec<(String, String, String)> = Vec::new(); // (name, type, value)
    let mut subkeys: Vec<String> = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let l = line.trim_end();
        if l.is_empty() { continue; }
        if l.ends_with("values:") || l.ends_with("子项:") {
            section = "values";
            continue;
        }
        if l.ends_with("subkeys:") || l.ends_with("子键:") {
            section = "subkeys";
            continue;
        }
        // reg query 英文输出: "Key has subkeys" / "End of search"
        let trimmed = l.trim();
        if trimmed.contains("REG_") {
            // 值行
            let parts: Vec<&str> = trimmed.splitn(2, "REG_").collect();
            if parts.len() == 2 {
                let name = parts[0].trim().to_string();
                let rest = format!("REG_{}", parts[1]);
                let toks: Vec<&str> = rest.split_whitespace().collect();
                let vtype = toks.get(0).map(|s| s.to_string()).unwrap_or_default();
                let vval = toks.get(1..).map(|s| s.join(" ")).unwrap_or_default();
                values.push((name, vtype, vval));
            }
        } else if section == "subkeys" || trimmed.starts_with("HKEY") {
            // 子键行(完整路径)
            let key = trimmed.rsplit('\\').next().unwrap_or(trimmed).to_string();
            if !key.contains("search") { subkeys.push(key); }
        }
    }
    if json {
        let obj = serde_json::json!({
            "path": path,
            "values": values.iter().map(|(n,t,v)| serde_json::json!({"name":n,"type":t,"value":v})).collect::<Vec<_>>(),
            "subkeys": subkeys,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else {
        println!("== {} ==", path);
        if !values.is_empty() {
            println!("\n值:");
            for (n, t, v) in &values {
                let disp = if n.is_empty() { "(默认)" } else { n.as_str() };
                println!("  {:<24} {:<10} {}", disp, t, v);
            }
        }
        if !subkeys.is_empty() {
            println!("\n子键:");
            for k in &subkeys {
                println!("  {}", k);
            }
        }
    }
    Ok(())
}

fn do_set(path: &str, name: Option<&str>, value: &str) -> anyhow::Result<()> {
    let nm = name.unwrap_or("");
    // 简单按 /t REG_SZ 处理;数字用 REG_DWORD
    let (t, v) = if let Ok(n) = value.parse::<i64>() {
        ("REG_DWORD", format!("{}", n))
    } else {
        ("REG_SZ", value.to_string())
    };
    let out = Command::new("reg").args(["add", path, "/v", nm, "/t", t, "/d", &v, "/f"]).output()
        .map_err(|e| anyhow::anyhow!("reg.exe 调用失败: {e}"))?;
    if out.status.success() {
        println!("✓ 已设置 {}\\{} = {} ({})", path, if nm.is_empty() {"(默认)"} else {nm}, v, t);
        Ok(())
    } else {
        anyhow::bail!("写入失败: {}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

fn do_delete(path: &str, name: Option<&str>) -> anyhow::Result<()> {
    let out = if let Some(nm) = name {
        Command::new("reg").args(["delete", path, "/v", nm, "/f"]).output()
    } else {
        Command::new("reg").args(["delete", path, "/f"]).output()
    }.map_err(|e| anyhow::anyhow!("reg.exe 调用失败: {e}"))?;
    if out.status.success() {
        println!("✓ 已删除 {}\\{}", path, name.unwrap_or("(整个键)"));
        Ok(())
    } else {
        anyhow::bail!("删除失败: {}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

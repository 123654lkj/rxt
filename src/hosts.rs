//! 远程主机配置管理

use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteOs {
    Linux,
    Windows,
}

impl Default for RemoteOs {
    fn default() -> Self { RemoteOs::Linux }
}

impl std::fmt::Display for RemoteOs {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RemoteOs::Linux => write!(f, "linux"),
            RemoteOs::Windows => write!(f, "windows"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostConfig {
    pub host: String,
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub password_env: Option<String>,
    #[serde(default)]
    pub os: Option<RemoteOs>,  // 可选，避免每次检测
    #[serde(default)]
    pub jump_host: Option<String>,  // v0.7.3: 跳板机 host alias (先 SSH 到此机, 再 direct-tcpip 到目标)
}

fn default_port() -> u16 { 22 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroupConfig {
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HostsFile {
    #[serde(default)]
    pub hosts: HashMap<String, HostConfig>,
    #[serde(default)]
    pub group: HashMap<String, GroupConfig>,
}

impl HostsFile {
    pub fn load() -> anyhow::Result<Self> {
        // 先注入 ~/.rxt/env，使 password_env 在 Agent/非交互壳里也能读到
        let _ = Self::load_dotenv();
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self {
                hosts: HashMap::new(),
                group: HashMap::new(),
            });
        }
        let content = std::fs::read_to_string(&path)?;
        let config: HostsFile = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home dir"))?;
        Ok(home.join(".rxt").join("hosts.toml"))
    }

    /// `~/.rxt/env` 路径（KEY=VALUE，chmod 600）
    pub fn env_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home dir"))?;
        Ok(home.join(".rxt").join("env"))
    }

    /// 加载 `~/.rxt/env` 到进程环境（已存在的 env 不覆盖，方便临时覆盖）。
    /// 格式：KEY=VALUE / KEY="VALUE" / # 注释 / 空行
    pub fn load_dotenv() -> anyhow::Result<usize> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Once;
        static ONCE: Once = Once::new();
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        ONCE.call_once(|| {
            let path = match Self::env_path() {
                Ok(p) => p,
                Err(_) => return,
            };
            if !path.exists() {
                return;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                return;
            };
            let mut n = 0usize;
            for line in content.lines() {
                let s = line.trim();
                if s.is_empty() || s.starts_with('#') {
                    continue;
                }
                let Some((k, v)) = s.split_once('=') else {
                    continue;
                };
                let k = k.trim();
                if k.is_empty() {
                    continue;
                }
                let mut v = v.trim().to_string();
                if (v.starts_with('"') && v.ends_with('"'))
                    || (v.starts_with('\'') && v.ends_with('\''))
                {
                    v = v[1..v.len() - 1].to_string();
                }
                // 不覆盖用户/父进程已设的环境变量
                if std::env::var_os(k).is_none() {
                    std::env::set_var(k, &v);
                    n += 1;
                }
            }
            COUNT.store(n, Ordering::Relaxed);
        });
        Ok(COUNT.load(Ordering::Relaxed))
    }

    pub fn get_host(&self, name: &str) -> anyhow::Result<&HostConfig> {
        let config_path = Self::config_path().unwrap_or_else(|_| PathBuf::from("~/.rxt/hosts.toml"));
        match self.hosts.get(name) {
            Some(h) => Ok(h),
            None => {
                let existing: Vec<String> = self.hosts.keys().cloned().collect();
                let existing_str = existing.join(", ");
                let msg = format!(
                    "Host not found: {}. Config file: {}. Existing hosts: {}",
                    name, config_path.display(), existing_str
                );
                Err(anyhow::anyhow!(msg))
            }
        }
    }

    pub fn get_group_members(&self, name: &str) -> anyhow::Result<Vec<String>> {
        match self.group.get(name) {
            Some(g) => Ok(g.members.clone()),
            None => {
                let existing: Vec<String> = self.group.keys().cloned().collect();
                let existing_str = existing.join(", ");
                let msg = format!(
                    "Group not found: {}. Existing groups: {}",
                    name, existing_str
                );
                Err(anyhow::anyhow!(msg))
            }
        }
    }

    /// 解析主机密码：**password_env 优先**，明文 `password` 仅作回退并告警一次。
    ///
    /// 安全策略（0.8.4+）：
    /// 1. 有 `password_env` 且环境变量有值 → 用 env（推荐）
    /// 2. 否则才用 hosts.toml 里的明文 `password`，并向 stderr 警告
    /// 3. 都没有 → None（由调用方走密钥或失败）
    pub fn get_password(&self, host: &HostConfig) -> Option<String> {
        // 1) 环境变量优先
        if let Some(env_var) = &host.password_env {
            match std::env::var(env_var) {
                Ok(v) if !v.is_empty() => return Some(v),
                Ok(_) => {
                    eprintln!(
                        "rxt: password_env={} 为空，尝试 hosts.toml 明文 password（不安全）",
                        env_var
                    );
                }
                Err(_) => {
                    // env 未设置：若有明文则回退并警告；若无则下面处理
                    if host.password.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
                        Self::warn_plaintext_once();
                        return host.password.clone();
                    }
                    eprintln!(
                        "rxt: password_env={} 未设置且无明文 password",
                        env_var
                    );
                    return None;
                }
            }
        }
        // 2) 明文回退（不推荐）
        if let Some(pass) = &host.password {
            if !pass.is_empty() {
                Self::warn_plaintext_once();
                return Some(pass.clone());
            }
        }
        None
    }

    fn warn_plaintext_once() {
        use std::sync::Once;
        static WARN: Once = Once::new();
        WARN.call_once(|| {
            eprintln!(
                "rxt: 警告 — hosts.toml 使用明文 password。建议改为 password_env=\"ENV_NAME\"，\n\
                 并把密码放进环境变量/Vaultwarden（bw-ai），勿把密钥写进配置文件。"
            );
        });
    }

    /// 认证方式摘要（**永不返回密码本身**），供 `rxt info` 脱敏展示。
    pub fn auth_summary(host: &HostConfig) -> &'static str {
        let has_key = host.key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
        let has_env = host.password_env.as_ref().map(|e| !e.is_empty()).unwrap_or(false);
        let has_plain = host.password.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
        match (has_key, has_env, has_plain) {
            (true, _, _) => "key",
            (false, true, true) => "password_env(+plaintext_fallback)",
            (false, true, false) => "password_env",
            (false, false, true) => "plaintext_password(INSECURE)",
            (false, false, false) => "none",
        }
    }
}

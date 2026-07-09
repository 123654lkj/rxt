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

    pub fn get_password(&self, host: &HostConfig) -> Option<String> {
        if let Some(pass) = &host.password {
            return Some(pass.clone());
        }
        if let Some(env_var) = &host.password_env {
            return std::env::var(env_var).ok();
        }
        None
    }
}

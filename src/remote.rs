//! 远程通道 — SSH/SFTP 连接管理
//! 让 AI 像管理本地一样管理远程服务器
//!
//! Feature 隔离:
//! - `remote` feature 开启时: 完整 SSH/SFTP 实现(依赖 ssh2 → OpenSSL)
//! - 关闭时: 提供桩(RemoteChannel::connect 永远报错),这样本地无 OpenSSL 也能编译出全功能本地版 rxt。

#[cfg(feature = "remote")]
mod imp {
    use std::path::{Path, PathBuf};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use ssh2::{Session, Sftp};

    use crate::hosts::{HostConfig, HostsFile};
    use crate::signature::FileSignature;

    pub struct RemoteChannel {
        session: Session,
        host_name: String,
        host_config: HostConfig,
    }

    impl RemoteChannel {
        /// 连接到远程主机
        pub fn connect(host_alias: &str) -> anyhow::Result<Self> {
            let hosts = HostsFile::load()?;
            let config = hosts.get_host(host_alias)?.clone();

            let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))?;
            let mut session = Session::new()?;
            session.set_tcp_stream(tcp);
            session.handshake()?;

            // 认证
            if let Some(key_path) = &config.key {
                let key = shellexpand::tilde(key_path).into_owned();
                session.userauth_pubkey_file(&config.user, None, Path::new(&key), None)?;
            } else if let Some(password) = hosts.get_password(&config) {
                session.userauth_password(&config.user, password)?;
            } else {
                // 尝试 agent 认证
                session.userauth_agent(&config.user)?;
            }

            if !session.authenticated() {
                anyhow::bail!("Authentication failed for {}@{}", config.user, config.host);
            }

            Ok(Self {
                session,
                host_name: host_alias.to_string(),
                host_config: config,
            })
        }

        /// 远程读取文件（原始字节）
        pub fn read_file(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            let sftp = self.session.sftp()?;
            let remote_path = path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
            let mut file = sftp.open(Path::new(remote_path))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            Ok(buf)
        }

        /// 远程读取文件（UTF-8 + 签名检测）
        pub fn read_file_utf8(&self, path: &Path) -> anyhow::Result<(String, FileSignature)> {
            let raw = self.read_file(path)?;
            let sig = FileSignature::detect(&raw);
            let text = crate::signature::to_utf8_lf(&raw, &sig);
            Ok((text, sig))
        }

        /// 远程写入文件（保持格式）
        pub fn write_file(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
            self.write_file_with_mode(path, content, 0o644)
        }

        /// 带权限的远程写文件
        pub fn write_file_with_mode(&self, path: &Path, content: &[u8], mode: i32) -> anyhow::Result<()> {
            let sftp = self.session.sftp()?;
            let remote_path = path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
            if let Some(parent) = Path::new(remote_path).parent() {
                let _ = self.mkdir_p(&sftp, parent);
            }
            let mut file = sftp.create(Path::new(remote_path))?;
            file.write_all(content)?;
            drop(file);
            // 单独用 exec 设置权限（避免 setstat API 复杂性）
            let _ = self.exec(&format!("chmod {:o} \"{}\"", mode, remote_path));
            Ok(())
        }

        /// 远程执行命令
        pub fn exec(&self, cmd: &str) -> anyhow::Result<String> {
            let mut channel = self.session.channel_session()?;
            channel.exec(cmd)?;

            let mut output = String::new();
            channel.read_to_string(&mut output)?;

            let exit_status = channel.exit_status()?;
            if exit_status != 0 {
                let mut stderr = String::new();
                let mut stderr_channel = channel.stderr();
                stderr_channel.read_to_string(&mut stderr)?;
                anyhow::bail!("Remote command failed (exit {}): {}", exit_status, stderr);
            }

            Ok(output)
        }

        /// 远程执行 rxt 命令
        pub fn exec_rxt(&self, args: &[&str]) -> anyhow::Result<String> {
            let cmd = format!("rxt {}", args.join(" "));
            self.exec(&cmd)
        }

        /// 检查远程 rxt 是否存在
        pub fn check_rxt(&self) -> anyhow::Result<bool> {
            match self.exec("which rxt") {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }

        /// 递归创建远程目录
        fn mkdir_p(&self, sftp: &Sftp, path: &Path) -> anyhow::Result<()> {
            let path_str = path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?;

            // 尝试直接创建
            match sftp.mkdir(path, 0o755) {
                Ok(_) => return Ok(()),
                Err(e) if e.code() == ssh2::ErrorCode::Session(-17) => {
                    // 目录已存在
                    return Ok(());
                }
                Err(_) => {}
            }

            // 递归创建父目录
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    self.mkdir_p(sftp, parent)?;
                }
            }

            // 再次尝试创建
            let _ = sftp.mkdir(path, 0o755);
            Ok(())
        }

        pub fn host_name(&self) -> &str {
            &self.host_name
        }

        pub fn host_config(&self) -> &HostConfig {
            &self.host_config
        }
    }
}

#[cfg(feature = "remote")]
pub use imp::RemoteChannel;

// ---- no-remote 桩: 本地编译模式,不存在远程能力 ----
#[cfg(not(feature = "remote"))]
mod stub {
    use std::path::Path;

    /// 桩: 本地编译模式下 RemoteChannel 不可用。
    /// 任何构造尝试都报错提示用户启用 remote feature 或使用远程版二进制。
    pub struct RemoteChannel {
        _private: (), // 不可外部构造
    }

    impl RemoteChannel {
        pub fn connect(_host_alias: &str) -> anyhow::Result<Self> {
            anyhow::bail!(
                "本 rxt 二进制未启用 remote 功能(编译时关闭了 `remote` feature)。\n\
                 如需 --host/--group 远程能力,请使用启用 remote 的版本(虎虎上编译的 rxt)。\n\
                 本地版仅支持本地文件操作。"
            );
        }

        pub fn read_file(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unreachable!("remote feature disabled")
        }
        pub fn read_file_utf8(&self, _path: &Path) -> anyhow::Result<(String, crate::signature::FileSignature)> {
            unreachable!("remote feature disabled")
        }
        pub fn write_file(&self, _path: &Path, _content: &[u8]) -> anyhow::Result<()> {
            unreachable!("remote feature disabled")
        }
        pub fn write_file_with_mode(&self, _path: &Path, _content: &[u8], _mode: i32) -> anyhow::Result<()> {
            unreachable!("remote feature disabled")
        }
        pub fn exec(&self, _cmd: &str) -> anyhow::Result<String> {
            unreachable!("remote feature disabled")
        }
        pub fn exec_rxt(&self, _args: &[&str]) -> anyhow::Result<String> {
            unreachable!("remote feature disabled")
        }
        pub fn check_rxt(&self) -> anyhow::Result<bool> {
            unreachable!("remote feature disabled")
        }
        pub fn host_name(&self) -> &str {
            unreachable!("remote feature disabled")
        }
        pub fn host_config(&self) -> &crate::hosts::HostConfig {
            unreachable!("remote feature disabled")
        }
    }
}

#[cfg(not(feature = "remote"))]
pub use stub::RemoteChannel;

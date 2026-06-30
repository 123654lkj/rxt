//! 远程通道 — SSH/SFTP 连接管理
//! 让 AI 像管理本地一样管理远程服务器
//!
//! v0.4.2: 从 ssh2(依赖 libssh2-sys → openssl-src, Windows 上 perl 路径 bug 编译困难)
//! 改用 russh(纯 Rust + ring 后端)。彻底摆脱 OpenSSL/perl/C 工具链依赖。
//! 接口保持不变: connect/exec/read_file/write_file/write_file_with_mode。
//!
//! Feature 隔离:
//! - `remote` feature 开启时: 完整 russh 实现
//! - 关闭时: 提供桩, 本地无任何 C 依赖也能编译

#[cfg(feature = "remote")]
mod imp {
    use std::path::Path;
    use std::sync::Arc;
    use anyhow::Context;
    use russh::{client, ChannelMsg};
    use russh::keys::*;

    use crate::hosts::{HostConfig, HostsFile};
    use crate::signature::FileSignature;

    /// russh client Handler (空实现, 接受所有主机密钥)
    struct ClientHandler;

    // russh 0.61 Handler trait 用原生 async fn (非 async_trait)
    impl client::Handler for ClientHandler {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &russh::keys::PublicKey,
        ) -> Result<bool, Self::Error> {
            // 接受所有主机密钥 (内网工具, 与原 ssh2 行为一致)
            Ok(true)
        }
    }

    pub struct RemoteChannel {
        rt: tokio::runtime::Runtime,
        handle: client::Handle<ClientHandler>,
        host_name: String,
        host_config: HostConfig,
    }

    impl RemoteChannel {
        pub fn connect(host_alias: &str) -> anyhow::Result<Self> {
            let hosts = HostsFile::load()?;
            let config = hosts.get_host(host_alias)?.clone();

            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?;

            let handle = rt.block_on(Self::connect_async(&config))?;

            Ok(Self { rt, handle, host_name: host_alias.to_string(), host_config: config })
        }

        async fn connect_async(config: &HostConfig) -> anyhow::Result<client::Handle<ClientHandler>> {
            let ssh_config = Arc::new(client::Config::default());
            let addr = format!("{}:{}", config.host, config.port);
            let mut handle = client::connect(ssh_config, &addr, ClientHandler)
                .await
                .context("SSH connect failed")?;

            // 认证: 优先密钥, 其次密码
            let authed = if let Some(key_path) = &config.key {
                let key_path = shellexpand::tilde(key_path).into_owned();
                let key_pair = load_secret_key(&key_path, None)
                    .with_context(|| format!("load key {}", key_path))?;
                // russh 0.61: authenticate_publickey 要 PrivateKeyWithHashAlg
                let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key_pair), None);
                handle.authenticate_publickey(&config.user, key_with_alg).await?
            } else if let Some(password) = HostsFile::load().ok().and_then(|h| h.get_password(config)) {
                handle.authenticate_password(&config.user, &password).await?
            } else {
                anyhow::bail!("No auth method (no key/password for {})", config.user);
            };

            if !authed.success() {
                anyhow::bail!("Authentication failed for {}@{}", config.user, config.host);
            }
            Ok(handle)
        }

        /// 远程执行命令, 返回合并的 stdout+stderr
        pub fn exec(&self, cmd: &str) -> anyhow::Result<String> {
            self.rt.block_on(async {
                let mut channel = self.handle.channel_open_session().await?;
                // russh 0.61: exec 签名 (want_reply, command)
                channel.exec(true, cmd).await?;

                let mut output = String::new();
                loop {
                    let Some(msg) = channel.wait().await else { break; };
                    match msg {
                        ChannelMsg::Data { ref data } | ChannelMsg::ExtendedData { ref data, .. } => {
                            output.push_str(&String::from_utf8_lossy(data));
                        }
                        ChannelMsg::ExitStatus { exit_status } => {
                            if exit_status != 0 {
                                anyhow::bail!("Remote command failed (exit {}): {}", exit_status, output);
                            }
                        }
                        ChannelMsg::Eof | ChannelMsg::Close => break,
                        _ => {}
                    }
                }
                Ok(output)
            })
        }

        pub fn read_file(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            // v0.4.2: 不用 SFTP (russh-sftp subsystem 协商在 russh 0.61 下会超时),
            // 改用 exec + base64 读文件。base64 保证二进制安全, 零转义问题。
            let cmd = format!("base64 -w0 \"{}\" 2>/dev/null || base64 \"{}\"", path.display(), path.display());
            let out = self.exec(&cmd)?;
            use base64::Engine;
            let cleaned = out.trim().replace(['\n', '\r'], "");
            Ok(base64::engine::general_purpose::STANDARD.decode(&cleaned)?)
        }

        pub fn read_file_utf8(&self, path: &Path) -> anyhow::Result<(String, FileSignature)> {
            let raw = self.read_file(path)?;
            let sig = FileSignature::detect(&raw);
            let text = crate::signature::to_utf8_lf(&raw, &sig);
            Ok((text, sig))
        }

        pub fn write_file(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
            self.write_file_with_mode(path, content, 0o644)
        }

        pub fn write_file_with_mode(&self, path: &Path, content: &[u8], mode: i32) -> anyhow::Result<()> {
            // v0.4.2: 不用 SFTP, 改用 exec + base64 写文件。
            // 内容 base64 编码后通过 base64 -d 解码写入, 二进制安全, 彻底避开 shell 引号转义。
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(content);
            let parent_cmd = if let Some(parent) = path.parent() {
                format!("mkdir -p \"{}\" && ", parent.display())
            } else { String::new() };
            // 用 printf '%s' <b64> | base64 -d > path  避免 base64 串过长被 argv 截断的风险
            let cmd = format!("{}printf '%s' '{}' | base64 -d > \"{}\" && chmod {:o} \"{}\"",
                parent_cmd, b64, path.display(), mode, path.display());
            self.exec(&cmd)?;
            Ok(())
        }

        pub fn host_name(&self) -> &str { &self.host_name }
        pub fn host_config(&self) -> &HostConfig { &self.host_config }

        pub fn exec_rxt(&self, args: &[&str]) -> anyhow::Result<String> {
            self.exec(&format!("rxt {}", args.join(" ")))
        }
        pub fn check_rxt(&self) -> anyhow::Result<bool> {
            match self.exec("which rxt") { Ok(_) => Ok(true), Err(_) => Ok(false) }
        }
    }
}

#[cfg(feature = "remote")]
pub use imp::RemoteChannel;

// ---- no-remote 桩 ----
#[cfg(not(feature = "remote"))]
mod stub {
    use std::path::Path;

    pub struct RemoteChannel { _private: () }

    impl RemoteChannel {
        pub fn connect(_host_alias: &str) -> anyhow::Result<Self> {
            anyhow::bail!(
                "本 rxt 二进制未启用 remote 功能(编译时关闭了 `remote` feature)。\n\
                 如需 --host/--group 远程能力,请使用启用 remote 的版本。\n\
                 本地版仅支持本地文件操作。"
            );
        }
        pub fn read_file(&self, _path: &Path) -> anyhow::Result<Vec<u8>> { unreachable!() }
        pub fn read_file_utf8(&self, _path: &Path) -> anyhow::Result<(String, crate::signature::FileSignature)> { unreachable!() }
        pub fn write_file(&self, _path: &Path, _content: &[u8]) -> anyhow::Result<()> { unreachable!() }
        pub fn write_file_with_mode(&self, _path: &Path, _content: &[u8], _mode: i32) -> anyhow::Result<()> { unreachable!() }
        pub fn exec(&self, _cmd: &str) -> anyhow::Result<String> { unreachable!() }
        pub fn exec_rxt(&self, _args: &[&str]) -> anyhow::Result<String> { unreachable!() }
        pub fn check_rxt(&self) -> anyhow::Result<bool> { unreachable!() }
        pub fn host_name(&self) -> &str { unreachable!() }
        pub fn host_config(&self) -> &crate::hosts::HostConfig { unreachable!() }
    }
}

#[cfg(not(feature = "remote"))]
pub use stub::RemoteChannel;

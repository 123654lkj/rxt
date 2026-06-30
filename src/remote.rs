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

    /// 把路径转成远程 shell 安全的参数。
    /// ~ 开头不加引号 (让 shell 展开 $HOME); 其它加双引号 (处理空格/特殊字符)。
    /// v0.4.3: 修复 ~ 路径被引号包裹导致不展开的 bug。
    fn shell_path(path: &Path) -> String {
        let s = path.to_string_lossy();
        if s.starts_with('~') {
            s.into_owned()
        } else {
            format!("\"{}\"", s)
        }
    }

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
            let (stdout, stderr, exit) = self.exec_sep(cmd)?;
            if exit != 0 {
                anyhow::bail!("Remote command failed (exit {}): {}", exit, stderr.trim());
            }
            Ok(stdout)
        }

        /// 分离 stdout/stderr 执行。返回 (stdout, stderr, exit_code)。
        /// v0.4.3: 修复 base64 解析被 stderr 污染的问题 —— 之前 exec 把 stderr 合并进 output,
        /// 导致 read_file 的 base64 输出混入 locale 警告解码失败。
        pub fn exec_sep(&self, cmd: &str) -> anyhow::Result<(String, String, i32)> {
            self.rt.block_on(async {
                let mut channel = self.handle.channel_open_session().await?;
                channel.exec(true, cmd).await?;

                let mut stdout = String::new();
                let mut stderr = String::new();
                let mut exit_code = 0i32;
                loop {
                    let Some(msg) = channel.wait().await else { break; };
                    match msg {
                        ChannelMsg::Data { ref data } => {
                            stdout.push_str(&String::from_utf8_lossy(data));
                        }
                        ChannelMsg::ExtendedData { ref data, .. } => {
                            stderr.push_str(&String::from_utf8_lossy(data));
                        }
                        ChannelMsg::ExitStatus { exit_status } => {
                            exit_code = exit_status as i32;
                        }
                        ChannelMsg::Eof | ChannelMsg::Close => break,
                        _ => {}
                    }
                }
                Ok((stdout, stderr, exit_code))
            })
        }

        pub fn read_file(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
            // v0.4.3: 用 exec_sep 只取 stdout, 彻底隔离 stderr (修复 locale 警告污染 base64)。
            // 用 cat | base64 而非 base64 -w0, 兼容 Linux/BSD/macOS; 解码前过滤所有空白。
            let p = shell_path(path);
            let cmd = format!("cat {} | base64", p);
            let (stdout, _stderr, exit) = self.exec_sep(&cmd)?;
            if exit != 0 {
                anyhow::bail!("read_file failed (exit {}): {}", exit, _stderr.trim());
            }
            use base64::Engine;
            let cleaned: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(&cleaned)
                .map_err(|e| anyhow::anyhow!("base64 decode failed: {} (raw len={}, first 60: {:?})", e, cleaned.len(), &cleaned[..cleaned.len().min(60)]))
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
            // v0.4.3: exec+base64 写文件, 二进制安全, 避开 shell 引号转义。
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(content);
            // 大于 ~1MB 走分块中转, 避免命令行 ARG_MAX 截断
            if b64.len() > 1_000_000 {
                return self.write_file_large(path, &b64, mode);
            }
            let p = shell_path(path);
            let parent = shell_path(path.parent().unwrap_or(Path::new(".")));
            let cmd = format!("mkdir -p {} && printf '%s' '{}' | base64 -d > {} && chmod {:o} {}",
                parent, b64, p, mode, p);
            let (_out, err, exit) = self.exec_sep(&cmd)?;
            if exit != 0 {
                anyhow::bail!("write_file failed (exit {}): {} | cmd: {}", exit, err.trim(), cmd);
            }
            Ok(())
        }

        /// 大文件: 分块通过 base64 文件中转写入, 突破 ARG_MAX 限制。
        fn write_file_large(&self, path: &Path, b64: &str, mode: i32) -> anyhow::Result<()> {
            let tmp = "/tmp/_rxt_write_large.b64";
            // 清空目标临时文件
            self.exec(&format!("rm -f {}", tmp))?;
            // 分块 append (每块 500KB, 避免 ARG_MAX)
            for chunk in b64.as_bytes().chunks(500_000) {
                let chunk_str = std::str::from_utf8(chunk)?;
                self.exec(&format!("printf '%s' '{}' >> {}", chunk_str, tmp))?;
            }
            // 解码写入目标 + 清理
            let p = shell_path(path);
            let cmd = format!("base64 -d {} > {} && chmod {:o} {} && rm -f {}",
                tmp, p, mode, p, tmp);
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

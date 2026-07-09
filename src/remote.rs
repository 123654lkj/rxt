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

    use crate::hosts::{HostConfig, HostsFile, RemoteOs};
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

    /// Windows pwsh path escape
    fn shell_path_win(path: &Path) -> String {
        let s = path.to_string_lossy().replace('/', "\\");
        format!("'{}'", s.replace("'", "''"))
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
        os: RemoteOs,
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

            let mut ch = Self { rt, handle, host_name: host_alias.to_string(), host_config: config.clone(), os: RemoteOs::Linux };
            ch.os = config.os.unwrap_or_else(|| ch.detect_os().unwrap_or(RemoteOs::Linux));
            Ok(ch)
        }

        /// 认证一个已连接的 SSH handle (密钥优先, 其次密码)。
        /// 跳板机和目标机复用同一套逻辑。
        async fn authenticate(
            handle: &mut client::Handle<ClientHandler>,
            config: &HostConfig,
            hosts: &HostsFile,
        ) -> anyhow::Result<()> {
            let authed = if let Some(key_path) = &config.key {
                let key_path = shellexpand::tilde(key_path).into_owned();
                let key_pair = load_secret_key(&key_path, None)
                    .with_context(|| format!("load key {}", key_path))?;
                // russh 0.61: authenticate_publickey 要 PrivateKeyWithHashAlg
                let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key_pair), None);
                handle.authenticate_publickey(&config.user, key_with_alg).await?
            } else if let Some(password) = hosts.get_password(config) {
                handle.authenticate_password(&config.user, &password).await?
            } else {
                anyhow::bail!("No auth method (no key/password for {})", config.user);
            };

            if !authed.success() {
                anyhow::bail!("Authentication failed for {}@{}", config.user, config.host);
            }
            Ok(())
        }

        async fn connect_async(config: &HostConfig) -> anyhow::Result<client::Handle<ClientHandler>> {
            let ssh_config = Arc::new(client::Config::default());
            let hosts = HostsFile::load()?;

            // v0.7.3: 跳板机模式 — 先 SSH 到 jump_host, 再 direct-tcpip 隧道到目标机
            if let Some(jump_alias) = &config.jump_host {
                // 1. 连接并认证跳板机
                let jump_config = hosts.get_host(jump_alias)
                    .with_context(|| format!("jump_host {} not found in hosts.toml", jump_alias))?
                    .clone();
                let jump_addr = format!("{}:{}", jump_config.host, jump_config.port);
                let mut jump_handle = client::connect(ssh_config.clone(), &jump_addr, ClientHandler)
                    .await
                    .with_context(|| format!("jump host SSH connect failed: {}", jump_addr))?;
                Self::authenticate(&mut jump_handle, &jump_config, &hosts).await
                    .context("jump host authentication failed")?;

                // 2. 通过跳板机开 direct-tcpip 隧道到目标机
                let channel = jump_handle
                    .channel_open_direct_tcpip(
                        &config.host, config.port as u32,
                        "127.0.0.1", 0,
                    )
                    .await
                    .with_context(|| format!("direct-tcpip to {}:{} via {} failed",
                        config.host, config.port, jump_alias))?;
                // Channel → ChannelStream (impl AsyncRead+AsyncWrite+Unpin+Send)
                let stream = channel.into_stream();

                // 3. 在隧道上建第二层 SSH 连接 + 认证目标机
                let mut handle = client::connect_stream(ssh_config, stream, ClientHandler)
                    .await
                    .with_context(|| format!("target SSH over tunnel failed: {}@{}:{}",
                        config.user, config.host, config.port))?;
                Self::authenticate(&mut handle, config, &hosts).await?;
                return Ok(handle);
            }

            // 直连模式 (原有逻辑)
            let addr = format!("{}:{}", config.host, config.port);
            let mut handle = client::connect(ssh_config, &addr, ClientHandler)
                .await
                .context("SSH connect failed")?;
            Self::authenticate(&mut handle, config, &hosts).await?;
            Ok(handle)
        }

        /// 远程执行命令, 返回合并的 stdout+stderr
        /// Detect remote OS
        fn detect_os(&self) -> anyhow::Result<RemoteOs> {
            // 先试 Linux: uname -s 返回 "Linux"
            if let Ok(out) = self.exec("uname -s 2>/dev/null") {
                if out.to_lowercase().contains("linux") { return Ok(RemoteOs::Linux); }
            }
            // 再试 Windows: 执行简单命令，Windows 的 cmd/PowerShell 都能 echo
            if let Ok(out) = self.exec("echo WIN") {
                if out.trim() == "WIN" { return Ok(RemoteOs::Windows); }
            }
            Ok(RemoteOs::Linux)
        }

        /// 解码远端输出: Windows 尝试 GBK 回退, Linux 用 UTF-8
        fn decode_remote(&self, data: &[u8]) -> String {
            match self.os {
                RemoteOs::Windows => {
                    // 先试 UTF-8, 如果有替换字符(说明不是有效UTF-8), 回退到 GBK
                    let utf8 = String::from_utf8_lossy(data);
                    if utf8.contains('\u{fffd}') {
                        // 有替换字符, 用 GBK 解码
                        encoding_rs::GBK.decode(data).0.into_owned()
                    } else {
                        utf8.into_owned()
                    }
                }
                _ => String::from_utf8_lossy(data).into_owned(),
            }
        }

        pub fn exec(&self, cmd: &str) -> anyhow::Result<String> {
            let (stdout, stderr, exit) = self.exec_sep(cmd)?;
            if exit != 0 {
                anyhow::bail!("Remote command failed (exit {}): {}", exit, stderr.trim());
            }
            Ok(stdout)
        }

        /// 远程是 Windows 吗?
        pub fn is_windows(&self) -> bool {
            matches!(self.os, RemoteOs::Windows)
        }

        /// 远程是 Linux 吗?
        pub fn is_linux(&self) -> bool {
            matches!(self.os, RemoteOs::Linux)
        }

        /// v0.4.4: Windows 不自动包装,用户需用 PowerShell 语法避免 GBK 乱码
        /// 例: hostname → $env:COMPUTERNAME, dir → Get-ChildItem
        fn wrap_cmd(&self, cmd: &str) -> String {
            cmd.to_string()
        }

        /// 分离 stdout/stderr 执行。返回 (stdout, stderr, exit_code)。
        /// v0.4.3: 修复 base64 解析被 stderr 污染的问题 —— 之前 exec 把 stderr 合并进 output,
        /// 导致 read_file 的 base64 输出混入 locale 警告解码失败。
        pub fn exec_sep(&self, cmd: &str) -> anyhow::Result<(String, String, i32)> {
            let cmd = self.wrap_cmd(cmd);
            self.rt.block_on(async {
                let mut channel = self.handle.channel_open_session().await?;
                channel.exec(true, cmd.as_bytes()).await?;

                let mut stdout = String::new();
                let mut stderr = String::new();
                let mut exit_code = 0i32;
                loop {
                    let Some(msg) = channel.wait().await else { break; };
                    match msg {
                        ChannelMsg::Data { ref data } => {
                            stdout.push_str(&self.decode_remote(data));
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
            use base64::Engine;
            match self.os {
                RemoteOs::Windows => {
                    let p = shell_path_win(path);
                    let cmd = format!("pwsh -NoProfile -Command \"[Convert]::ToBase64String([IO.File]::ReadAllBytes({}))\"", p);
                    let (stdout, stderr, exit) = self.exec_sep(&cmd)?;
                    if exit != 0 { anyhow::bail!("read_file (win) exit {}: {}", exit, stderr.trim()); }
                    let cleaned: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();
                    base64::engine::general_purpose::STANDARD.decode(&cleaned)
                        .map_err(|e| anyhow::anyhow!("base64 decode failed: {} (len={})", e, cleaned.len()))
                }
                _ => {
                    let p = shell_path(path);
                    let cmd = format!("cat {} | base64", p);
                    let (stdout, stderr, exit) = self.exec_sep(&cmd)?;
                    if exit != 0 { anyhow::bail!("read_file exit {}: {}", exit, stderr.trim()); }
                    let cleaned: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();
                    base64::engine::general_purpose::STANDARD.decode(&cleaned)
                        .map_err(|e| anyhow::anyhow!("base64 decode failed: {} (len={})", e, cleaned.len()))
                }
            }
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
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(content);
            if b64.len() > 1_000_000 {
                return self.write_file_large(path, &b64, mode);
            }
            match self.os {
                RemoteOs::Windows => {
                    let p = shell_path_win(path);
                    // Extract parent manually since Linux Path doesn't understand Windows paths
                    let path_str = path.to_string_lossy().replace('\\', "/");
                    let parent_str = if let Some(idx) = path_str.rfind('/') {
                        &path_str[..idx]
                    } else {
                        "."
                    };
                    let parent = shell_path_win(std::path::Path::new(parent_str));
                    let cmd = format!(
                        "pwsh -NoProfile -Command \"[IO.Directory]::CreateDirectory({}); [IO.File]::WriteAllBytes({}, [Convert]::FromBase64String(\'{}\'))\"",
                        parent, p, b64
                    );
                    let (_out, err, exit) = self.exec_sep(&cmd)?;
                    if exit != 0 { anyhow::bail!("write_file (win) exit {}: {}", exit, err.trim()); }
                    Ok(())
                }
                _ => {
                    let p = shell_path(path);
                    let parent = shell_path(path.parent().unwrap_or(Path::new(".")));
                    let cmd = format!("mkdir -p {} && printf '%s' '{}' | base64 -d > {} && chmod {:o} {}",
                        parent, b64, p, mode, p);
                    let (_out, err, exit) = self.exec_sep(&cmd)?;
                    if exit != 0 { anyhow::bail!("write_file exit {}: {}", exit, err.trim()); }
                    Ok(())
                }
            }
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
        pub fn remote_os(&self) -> RemoteOs { self.os }

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
        pub fn remote_os(&self) -> crate::hosts::RemoteOs { unreachable!() }
    }
}

#[cfg(not(feature = "remote"))]
pub use stub::RemoteChannel;
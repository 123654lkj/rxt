//! Storage — 统一本地/远程文件操作 (v0.6.1)
//!
//! 消灭各模块的 if let Some(remote) 重复分支。
//! 模块只调 storage.read_file()，不关心数据从哪来。

use crate::remote::RemoteChannel;
use crate::signature::{to_utf8_lf, FileSignature};
use std::path::Path;

/// 统一存储接口: 本地或远程
pub enum Storage<'a> {
    Local,
    Remote(&'a RemoteChannel),
}

impl<'a> Storage<'a> {
    /// 读文件原始字节
    pub fn read_file(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        match self {
            Storage::Local => Ok(std::fs::read(path)?),
            Storage::Remote(rc) => rc.read_file(path),
        }
    }

    /// 读文件 + 编码检测 → UTF-8 LF 文本
    pub fn read_text(&self, path: &Path) -> anyhow::Result<(String, FileSignature)> {
        match self {
            Storage::Local => {
                let raw = std::fs::read(path)?;
                let sig = FileSignature::detect(&raw);
                Ok((to_utf8_lf(&raw, &sig), sig))
            }
            Storage::Remote(rc) => rc.read_file_utf8(path),
        }
    }

    /// 写文件 (保持格式)
    pub fn write_file(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
        match self {
            Storage::Local => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                Ok(std::fs::write(path, content)?)
            }
            Storage::Remote(rc) => rc.write_file(path, content),
        }
    }

    /// 写文件 + 指定权限
    pub fn write_file_with_mode(
        &self,
        path: &Path,
        content: &[u8],
        mode: i32,
    ) -> anyhow::Result<()> {
        match self {
            Storage::Local => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                std::fs::write(path, content)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode as u32))?;
                }
                Ok(())
            }
            Storage::Remote(rc) => rc.write_file_with_mode(path, content, mode),
        }
    }

    /// 判断是否远程
    pub fn is_remote(&self) -> bool {
        matches!(self, Storage::Remote(_))
    }

    /// 获取远程引用 (如果是远程)
    pub fn remote(&self) -> Option<&RemoteChannel> {
        match self {
            Storage::Remote(rc) => Some(rc),
            _ => None,
        }
    }

    /// 从 Option<&RemoteChannel> 创建 Storage
    pub fn from_remote(remote: Option<&'a RemoteChannel>) -> Self {
        match remote {
            Some(rc) => Storage::Remote(rc),
            None => Storage::Local,
        }
    }
}

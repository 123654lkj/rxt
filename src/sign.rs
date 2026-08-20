//! Windows Authenticode 签名（自签 CN=rxt-codesign）
//! 非 Windows：提示无需签名。

use std::path::{Path, PathBuf};

pub fn cer_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rxt")
        .join("rxt-codesign.cer")
}

pub fn run(exe: Option<&Path>, trust: bool) -> anyhow::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = (exe, trust);
        println!("签名仅 Windows 需要。");
        return Ok(());
    }
    #[cfg(windows)]
    {
        let path = match exe {
            Some(p) => p.to_path_buf(),
            None => std::env::current_exe()?,
        };
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        let resolved = std::fs::canonicalize(&path)
            .map_err(|e| anyhow::anyhow!("找不到 {}: {e}", path.display()))?;
        let current = std::env::current_exe()?;
        if resolved == std::fs::canonicalize(&current)? {
            sign_current_exe(&path, trust)
        } else {
            sign_path(&path, trust)
        }
    }
}

/// 当前 exe 正在占用：先签副本，再用已签副本原位替换。
#[cfg(windows)]
fn sign_current_exe(exe: &Path, trust: bool) -> anyhow::Result<()> {
    let file = exe
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| anyhow::anyhow!("非法 exe 路径: {}", exe.display()))?;
    let pid = std::process::id();
    let signed = exe.with_file_name(format!("{file}.{pid}.signed.exe"));
    let old = exe.with_file_name(format!("{file}.{pid}.old"));

    std::fs::copy(exe, &signed)?;
    if let Err(e) = sign_path(&signed, trust) {
        let _ = std::fs::remove_file(&signed);
        return Err(e);
    }

    if let Err(e) = std::fs::rename(exe, &old) {
        let _ = std::fs::remove_file(&signed);
        anyhow::bail!(
            "已签副本，但无法替换正在运行的 {}: {e}。请用另一份 rxt 执行 `rxt sign {}`",
            exe.display(),
            exe.display()
        );
    }
    if let Err(e) = std::fs::rename(&signed, exe) {
        let _ = std::fs::copy(&old, exe);
        anyhow::bail!("已签副本替换失败，已尝试恢复原文件: {e}");
    }
    eprintln!("# 当前程序已替换为签名副本；旧文件 {}", old.display());
    Ok(())
}

/// 签名 exe；trust=true 时尝试导入 TrustedPublisher
#[cfg(windows)]
pub fn sign_path(exe: &Path, trust: bool) -> anyhow::Result<()> {
    if !exe.is_file() {
        anyhow::bail!("不是文件: {}", exe.display());
    }
    let exe_s = exe.to_string_lossy().replace('\'', "''");
    let cer = cer_path();
    let cer_s = cer.to_string_lossy().replace('\'', "''");
    let dir_s = cer
        .parent()
        .map(|p| p.to_string_lossy().replace('\'', "''"))
        .unwrap_or_default();
    let trust_block = if trust {
        format!(
            "Import-Certificate -FilePath '{cer_s}' -CertStoreLocation \
             Cert:\\CurrentUser\\TrustedPublisher | Out-Null\n\
             Write-Output 'RXT_TRUSTED=true'"
        )
    } else {
        String::new()
    };
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$exe = '{exe_s}'
$cer = '{cer_s}'
New-Item -ItemType Directory -Force -Path '{dir_s}' | Out-Null
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object {{
  $_.Subject -eq 'CN=rxt-codesign' -and
  $_.HasPrivateKey -and
  $_.NotAfter -gt (Get-Date) -and
  $_.EnhancedKeyUsageList.ObjectId -contains '1.3.6.1.5.5.7.3.3'
}} | Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {{
  $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=rxt-codesign' `
    -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(10)
}}
Export-Certificate -Cert $cert -FilePath $cer -Force | Out-Null
{trust_block}
for ($i = 0; $i -lt 20; $i++) {{
  try {{
    $signed = Set-AuthenticodeSignature -FilePath $exe -Certificate $cert -ErrorAction Stop
    if (
      $null -eq $signed -or
      [string]::IsNullOrWhiteSpace([string]$signed.Status) -or
      [string]$signed.Status -eq 'NotSigned'
    ) {{ throw 'Set-AuthenticodeSignature 没有写入签名' }}
    break
  }} catch {{
    if ($i -eq 19) {{ throw }}
    Start-Sleep -Milliseconds 250
  }}
}}
$sig = Get-AuthenticodeSignature -LiteralPath $exe
if ($null -eq $sig) {{ throw 'Get-AuthenticodeSignature 没有返回签名状态' }}
Write-Output ('RXT_STATUS=' + [string]$sig.Status)
Write-Output ('RXT_THUMBPRINT=' + $cert.Thumbprint)
"#
    );
    let ps_args = [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ];
    let out = std::process::Command::new("pwsh")
        .args(ps_args)
        .output()
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                std::process::Command::new("powershell")
                    .args(ps_args)
                    .output()
            } else {
                Err(e)
            }
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        anyhow::bail!("签名失败: {}\n{}", stdout.trim(), stderr.trim());
    }
    let status = stdout
        .lines()
        .find_map(|line| {
            line.trim_start_matches('\u{feff}')
                .strip_prefix("RXT_STATUS=")
        })
        .ok_or_else(|| anyhow::anyhow!("签名命令没有返回状态: {}", stdout.trim()))?;
    if !matches!(status, "Valid" | "UnknownError") {
        anyhow::bail!("签名状态异常: {status}\n{}", stdout.trim());
    }
    eprintln!(
        "# 已签名 {}（{}）证书 {}",
        exe.display(),
        status,
        cer.display()
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn sign_path(_exe: &Path, _trust: bool) -> anyhow::Result<()> {
    Ok(())
}

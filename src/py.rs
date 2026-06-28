use std::path::PathBuf;
use std::io::Read;
use std::process::{Command, Stdio};

/// 执行内联 Python — 替代 PowerShel 模板的 8 行垫脚石
pub fn run(code: Option<&str>, file: Option<&PathBuf>) -> anyhow::Result<()> {
    let tmp_dir = std::env::temp_dir();
    let py_path = tmp_dir.join("_rxt_py.py");

    let source = if let Some(f) = file {
        std::fs::read_to_string(f)?
    } else if let Some(c) = code {
        c.to_string()
    } else {
        // Read from stdin
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    };

    // Write UTF-8 no BOM
    std::fs::write(&py_path, source.as_bytes())?;

    let python = if cfg!(windows) { r"C:\Program Files\Python311\python.exe" } else { "python3" };
    let output = Command::new(python)
        .arg(&py_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()?;

    let _ = std::fs::remove_file(&py_path);
    std::process::exit(output.status.code().unwrap_or(1))
}

use std::path::{Path, PathBuf};
use std::io::Read;
use std::process::{Command, Stdio};

fn python_path() -> String {
    if cfg!(windows) { "C:/Progra~1/Python311/python.exe".to_string() }
    else { "python3".to_string() }
}

/// 解释器配置
struct Runner {
    suffix: &'static str,
    program: &'static str,
    base_args: &'static [&'static str],
    use_stdin: bool,
}

const RUNNER_SH: Runner = Runner { suffix: "sh", program: "sh", base_args: &[], use_stdin: true };
const RUNNER_PY: Runner = Runner { suffix: "py", program: "python3", base_args: &["-c"], use_stdin: true };
const RUNNER_PS1: Runner = Runner { suffix: "ps1", program: "powershell", base_args: &["-NoProfile", "-Command"], use_stdin: true };
const RUNNER_BAT: Runner = Runner { suffix: "bat", program: "cmd", base_args: &["/C"], use_stdin: false };

/// 智能检测:默认是 sh
fn auto_detect_lang(code: &str) -> Option<&'static str> {
    let trimmed = code.trim_start();
    if trimmed.starts_with("#!/") || trimmed.contains("\n#!") {
        if trimmed.contains("python") { return Some("py"); }
        if trimmed.contains("bash") || trimmed.contains("/sh") { return Some("sh"); }
        if trimmed.contains("pwsh") || trimmed.contains("powershell") { return Some("ps1"); }
    }
    let first_line = code.lines().next().unwrap_or("");
    if first_line.starts_with("import ") || first_line.starts_with("from ")
        || first_line.starts_with("def ") || first_line.starts_with("class ")
        || first_line.starts_with("print(") || first_line.starts_with("if __name__")
        || code.contains("\ndef ") || code.contains("\nclass ") {
        return Some("py");
    }
    if first_line.contains("param(") || first_line.starts_with("$") {
        return Some("ps1");
    }
    Some("sh")
}

fn runner_for(lang: &str) -> &'static Runner {
    match lang {
        "sh" | "bash" | "zsh" => &RUNNER_SH,
        "py" | "python" => &RUNNER_PY,
        "ps1" | "powershell" | "pwsh" => &RUNNER_PS1,
        "bat" | "cmd" => &RUNNER_BAT,
        _ => &RUNNER_SH,
    }
}

pub fn run(
    code: &str,
    b64: bool,
    lang: Option<&str>,
    write_to: Option<&PathBuf>,
    remote: Option<&crate::remote::RemoteChannel>,
    login: bool,
    json_output: bool,
) -> anyhow::Result<i32> {
    let code_owned;
    let code = if code.is_empty() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        code_owned = buf;
        &code_owned
    } else {
        code
    };

    let decoded: Vec<u8> = if b64 {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(code.trim())
            .map_err(|e| anyhow::anyhow!("base64 decode error: {}", e))?
    } else {
        code.as_bytes().to_vec()
    };

    if let Some(path) = write_to {
        std::fs::write(path, &decoded)?;
        let size = decoded.len();
        let label = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        println!("  wrote {} bytes -> {}", size, label);
        return Ok(0);
    }

    let text = String::from_utf8(decoded)
        .map_err(|_| anyhow::anyhow!("decoded content is not valid UTF-8"))?;

    let detected = lang
        .map(|l| if l == "auto" { auto_detect_lang(&text).unwrap_or("sh") } else { l })
        .unwrap_or_else(|| auto_detect_lang(&text).unwrap_or("sh"));

    if let Some(remote_channel) = remote {
        return run_remote(&text, detected, login, json_output, remote_channel);
    }

    run_local(&text, runner_for(detected), login, json_output)
}

fn run_local(text: &str, runner: &Runner, login: bool, json_output: bool) -> anyhow::Result<i32> {
    let program = if runner.program == "python3" && cfg!(windows) {
        python_path()
    } else {
        runner.program.to_string()
    };

    let (final_program, extra_args) = if login && (program == "sh" || program == "bash") {
        ("bash".to_string(), vec!["-l".to_string()])
    } else {
        (program, vec![])
    };

    let mut cmd = Command::new(&final_program);
    cmd.args(&extra_args);

    if runner.use_stdin {
        // stdin 模式:代码走管道,不要 base_args
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if json_output {
            let json = serde_json::json!({
                "exit_code": status.code().unwrap_or(-1),
                "stdout": "",
                "stderr": "",
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        return Ok(status.code().unwrap_or(1));
    } else {
        // 文件模式
        for a in runner.base_args {
            cmd.arg(a);
        }
        let tmp_file = std::env::temp_dir().join(format!("_rxt_exec.{}", runner.suffix));
        std::fs::write(&tmp_file, text.as_bytes())?;
        cmd.arg(tmp_file.to_string_lossy().to_string());
        let status = cmd.status()?;
        let _ = std::fs::remove_file(&tmp_file);
        if json_output {
            let json = serde_json::json!({
                "exit_code": status.code().unwrap_or(-1),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        return Ok(status.code().unwrap_or(1));
    }
}

fn run_remote(text: &str, lang: &str, login: bool, json_output: bool, remote: &crate::remote::RemoteChannel) -> anyhow::Result<i32> {
    let runner = runner_for(lang);
    let tmp_path = format!("/tmp/_rxt_exec_remote.{}", runner.suffix);
    let tmp_file = Path::new(&tmp_path);

    remote.write_file_with_mode(tmp_file, text.as_bytes(), 0o755)?;

    // 远端:临时文件是脚本,直接 program tmp_path
    let login_prefix = if login { "bash -lc " } else { "" };
    let cmd = format!("{} {} {}", login_prefix.trim_end(), runner.program, tmp_path);

    let output = remote.exec(&cmd)?;
    print!("{}", output);

    let _ = remote.exec(&format!("rm -f {}", tmp_path));
    if json_output {
        let json = serde_json::json!({
            "exit_code": 0,
            "stdout": output,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    Ok(0)
}

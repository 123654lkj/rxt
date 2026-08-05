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
const RUNNER_PWSH7: Runner = Runner { suffix: "ps1", program: "pwsh", base_args: &["-NoProfile", "-Command"], use_stdin: true };
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
    // SQL 检测: psql 元命令或常见 SQL 关键字开头
    let fl_lower = first_line.to_lowercase();
    if first_line.starts_with("\\") // psql 元命令 \echo \d \dt 等
        || fl_lower.starts_with("select ") || fl_lower.starts_with("with ")
        || fl_lower.starts_with("explain ") || fl_lower.starts_with("create ")
        || fl_lower.starts_with("insert ") || fl_lower.starts_with("update ")
        || fl_lower.starts_with("delete ") || fl_lower.starts_with("alter ")
        || fl_lower.starts_with("-- ") && code.to_lowercase().contains("select ") {
        return Some("sql");
    }
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
        "ps1" | "powershell" => &RUNNER_PS1,
        "pwsh" | "pwsh7" => &RUNNER_PWSH7,
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
    container: Option<&str>,
    db: Option<&str>,
    sql_user: Option<&str>,
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

    // v0.4.1 集成A+B: 容器直达 + SQL 免转义执行
    // SQL 优先判断 (lang=sql): 通过 psql stdin 执行, 彻底避开 shell 引号转义地狱。
    // 容器场景 (container=NAME): docker exec -i NAME psql ... 或 docker exec -i NAME sh。
    if detected == "sql" {
        return run_sql(&text, remote, container, db, sql_user, json_output);
    }

    // 容器直达 (非 SQL): 把脚本通过 docker exec -i <container> sh 管道执行
    if let Some(cname) = container {
        return run_in_container(&text, detected, cname, remote, login, json_output);
    }

    if let Some(remote_channel) = remote {
        return run_remote(&text, detected, login, json_output, remote_channel);
    }

    run_local(&text, runner_for(detected), login, json_output)
}

/// v0.4.1 集成B: SQL 免转义执行。
/// 通过 psql 的 stdin 管道传入 SQL 文本, 彻底避开 shell 引号/换行转义问题。
/// 三种场景:
///   1. 本地直接 psql: `psql -U user -d db` (本地装了 psql)
///   2. 本地容器: `docker exec -i CONTAINER psql -U user -d db`
///   3. 远程容器: SSH 执行远程主机上的 `docker exec -i CONTAINER psql ...`
fn run_sql(
    sql: &str,
    remote: Option<&crate::remote::RemoteChannel>,
    container: Option<&str>,
    db: Option<&str>,
    sql_user: Option<&str>,
    json_output: bool,
) -> anyhow::Result<i32> {
    let user = sql_user.unwrap_or("postgres");
    let dbname = db.unwrap_or("postgres");

    // 构造 psql 命令前缀
    let psql_cmd = format!("psql -U {} -d {} -v ON_ERROR_STOP=1", user, dbname);

    // 完整执行命令 (含容器包裹)
    let full_cmd = if let Some(cname) = container {
        format!("docker exec -i {} {}", cname, psql_cmd)
    } else {
        psql_cmd.clone()
    };

    // 远程: 通过 SSH channel 执行, SQL 内容走远程临时文件 (避免 SSH 命令行的引号地狱)
    if let Some(remote_channel) = remote {
        let tmp = "/tmp/_rxt_sql.sql";
        remote_channel.write_file(std::path::Path::new(tmp), sql.as_bytes())?;
        let remote_cmd = if container.is_some() {
            // 远程容器: 先 cp SQL 进容器, 再 psql -f
            if let Some(cname) = container {
                format!("docker cp {} {}:{}_in.sql && docker exec {} {} -f {}_in.sql && docker exec {} rm -f {}_in.sql",
                        tmp, cname, tmp, cname, psql_cmd, tmp, cname, tmp)
            } else {
                format!("{} -f {}", psql_cmd, tmp)
            }
        } else {
            format!("{} -f {}", psql_cmd, tmp)
        };
        let output = remote_channel.exec(&remote_cmd)?;
        print!("{}", output);
        let _ = remote_channel.exec(&format!("rm -f {}", tmp));
        if json_output {
            let json = serde_json::json!({"exit_code": 0, "stdout": output});
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        return Ok(0);
    }

    // 本地: SQL 内容走 stdin 管道, 零转义
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&full_cmd);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(sql.as_bytes())?;
    }
    let status = child.wait()?;
    if json_output {
        let json = serde_json::json!({"exit_code": status.code().unwrap_or(-1)});
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    Ok(status.code().unwrap_or(1))
}

/// v0.4.1 集成A: 容器直达执行。
/// 把脚本通过 `docker exec -i CONTAINER sh` (或对应 runner) 管道执行。
/// 远程时在远程主机上执行 docker exec。
fn run_in_container(
    text: &str,
    lang: &str,
    container: &str,
    remote: Option<&crate::remote::RemoteChannel>,
    login: bool,
    json_output: bool,
) -> anyhow::Result<i32> {
    let runner = runner_for(lang);
    let inner_program = if runner.program == "python3" && cfg!(windows) {
        python_path()
    } else {
        runner.program.to_string()
    };

    // 容器内执行命令: docker exec -i CONTAINER <program>
    // shell 类用 stdin 管道, 其它写临时文件
    let use_pipe = matches!(lang, "sh" | "bash" | "zsh");

    // 远程容器
    if let Some(remote_channel) = remote {
        if use_pipe {
            // 远程 + shell: 写远程临时脚本, docker exec 跑它
            let tmp = "/tmp/_rxt_container.sh";
            remote_channel.write_file_with_mode(std::path::Path::new(tmp), text.as_bytes(), 0o755)?;
            let cmd = format!("docker exec -i {} {} {}", container, inner_program, tmp);
            let output = remote_channel.exec(&cmd)?;
            print!("{}", output);
            let _ = remote_channel.exec(&format!("rm -f {}", tmp));
            if json_output {
                let json = serde_json::json!({"exit_code": 0, "stdout": output});
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            return Ok(0);
        }
        // 非 shell 远程: 暂回退到主机层执行 (容器内非 shell 场景较少)
        return run_remote(text, lang, login, json_output, remote_channel);
    }

    // 本地容器: docker exec -i CONTAINER <program>, 脚本走 stdin
    let full_cmd = format!("docker exec -i {} {}", container, inner_program);
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&full_cmd);
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
        let json = serde_json::json!({"exit_code": status.code().unwrap_or(-1)});
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    Ok(status.code().unwrap_or(1))
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
    let os = remote.remote_os();
    
    let (tmp_path, exec_cmd, rm_cmd) = match os {
        crate::hosts::RemoteOs::Windows => {
            // 用系统自带 Windows PowerShell 5.1 全路径（避免 pwsh 未装 / PS7 坏 shim）
            let ps = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
            let tmp = format!("C:\\Users\\{}\\AppData\\Local\\Temp\\_rxt_exec.ps1", remote.host_config().user);
            // -ExecutionPolicy Bypass：远端默认 Restricted 时禁止跑 .ps1
            let exec = format!("{} -NoProfile -ExecutionPolicy Bypass -Command \"[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::InputEncoding=[Text.Encoding]::UTF8; & '{}'\"", ps, tmp);
            let rm = format!("{} -NoProfile -ExecutionPolicy Bypass -Command \"Remove-Item \'{}\'  -Force -ErrorAction SilentlyContinue\"", ps, tmp);
            (tmp, exec, rm)
        }
        _ => {
            let tmp = format!("/tmp/_rxt_exec.{}", runner.suffix);
            let login_prefix = if login { "bash -lc " } else { "" };
            let exec = format!("{} {} {}", login_prefix.trim_end(), runner.program, tmp);
            let rm = format!("rm -f {}", tmp);
            (tmp, exec, rm)
        }
    };

    let tmp_file = Path::new(&tmp_path);
    remote.write_file_with_mode(tmp_file, text.as_bytes(), 0o755)?;

    let output = remote.exec(&exec_cmd)?;
    print!("{}", output);

    let _ = remote.exec(&rm_cmd);
    if json_output {
        let json = serde_json::json!({
            "exit_code": 0,
            "stdout": output,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    Ok(0)
}

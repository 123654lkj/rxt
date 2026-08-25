//! rxt 核心宿主 — 最少内置命令。其余全部走插件（rxt-tools / 用户外挂）。

use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

use crate::plugin::{self, PluginCli};

pub const CORE_COMMANDS: &[&str] = &[
    "plugin", "exec", "info", "version", "upgrade", "deploy", "publish", "sign",
];

#[derive(Parser)]
#[command(
    name = "rxt",
    version,
    about = "rxt 核心宿主 — 插件调度；业务命令全部外挂可装卸",
    after_help = "业务命令（pack/grep/mem/http…）已拆成插件。\n  rxt plugin seed          安装官方标准库\n  rxt plugin list          看已装插件\n  rxt plugin remove grep   卸载某一个\n  rxt plugin add grep      再装回来（不必重编 rxt）",
    allow_external_subcommands = true
)]
pub struct Cli {
    #[arg(long, global = true, help = "远程主机（从 ~/.rxt/hosts.toml 读取）")]
    pub host: Option<String>,
    #[arg(long, global = true, help = "远程主机组（批量执行）")]
    pub group: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Clone)]
pub enum Command {
    #[command(about = "外挂插件 — new/add/install/seed/edit/show/remove/which")]
    Plugin {
        #[arg(
            help = "list | new | add | install | seed | edit | show | remove | which",
            default_value = "list"
        )]
        action: String,
        #[arg(help = "插件名，或 install 的路径")]
        target: Option<String>,
        #[arg(help = "new/add 时的脚本正文")]
        content: Option<String>,
        #[arg(long, help = "install 时指定名称；new 时覆盖名字")]
        name: Option<String>,
        #[arg(long, help = "覆盖同名内置，或覆盖已安装")]
        force: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, help = "new 语言: sh|py|cmd|ps1")]
        lang: Option<String>,
        #[arg(long, help = "脚本正文")]
        body: Option<String>,
        #[arg(long, help = "从 stdin 读脚本正文")]
        stdin: bool,
        #[arg(long, help = "new 之后打开编辑器")]
        open: bool,
    },
    #[command(about = "多语言代码执行")]
    Exec {
        code: Option<String>,
        #[arg(long)]
        b64: bool,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, help = "走 login shell")]
        login: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, help = "在 docker 容器内执行")]
        container: Option<String>,
        #[arg(long, help = "SQL 数据库名")]
        db: Option<String>,
        #[arg(long, help = "SQL 用户名")]
        sql_user: Option<String>,
    },
    #[command(about = "rxt 自检 - 版本/配置/hosts 状态")]
    Info {
        #[arg(long)]
        json: bool,
    },
    #[command(about = "批量查询版本 + 一致性检测")]
    Version {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        host: Option<String>,
    },
    #[command(about = "自我更新 — git pull + 编译 + 热替换")]
    Upgrade {
        #[arg(long)]
        repo: Option<String>,
        #[arg(long, help = "只检查不升级")]
        check: bool,
        #[arg(long)]
        features: Option<String>,
        #[arg(long, help = "只 pull 不编译")]
        no_build: bool,
    },
    #[command(about = "部署二进制到远程机器")]
    Deploy {
        binary: PathBuf,
        #[arg(short = 't', long = "to")]
        to: Vec<String>,
        #[arg(long)]
        all: bool,
        #[arg(long = "remote-path")]
        remote_path: Option<String>,
    },
    #[command(about = "一键发布 — 编译两平台 + 装核心与标准库插件 + 部署")]
    Publish {
        #[arg(long)]
        repo: Option<String>,
        #[arg(long, help = "不部署远程机器")]
        no_deploy: bool,
        #[arg(long, help = "不 git push")]
        no_push: bool,
        #[arg(short, long)]
        message: Option<String>,
    },
    #[command(about = "Windows 代码签名（自签 rxt-codesign）")]
    Sign {
        exe: Option<PathBuf>,
        #[arg(long)]
        trust: bool,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

pub fn parse_cli(args: Vec<String>) -> anyhow::Result<Result<Cli, clap::Error>> {
    std::thread::Builder::new()
        .name("rxt-clap".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || Cli::try_parse_from(args))
        .map_err(|e| anyhow::anyhow!("启动 CLI 解析线程失败: {e}"))?
        .join()
        .map_err(|_| anyhow::anyhow!("CLI 解析线程异常退出"))
}

pub fn run() -> anyhow::Result<()> {
    crate::common::setup_utf8_console();
    let _ = crate::hosts::HostsFile::load_dotenv();

    let mut args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == args[0] {
        args.remove(1);
    }
    if args.iter().any(|a| a == "--describe") {
        return describe_merged();
    }

    let _ = plugin::auto_seed();

    let raw_host = plugin::peek_flag(&args, "--host");
    let raw_group = plugin::peek_flag(&args, "--group");
    if let Some((name, rest)) = plugin::peek_subcommand(&args) {
        let rest = plugin::strip_global_flags(&rest);
        if plugin::run_forced_override(&name, &rest, raw_host.as_deref(), raw_group.as_deref())? {
            return Ok(());
        }
    }

    let cli = match parse_cli(args)? {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    match &cli.command {
        Command::Deploy {
            binary,
            to,
            all,
            remote_path,
        } => {
            let (targets, is_group) = if *all {
                (vec!["all".to_string()], true)
            } else {
                (to.clone(), false)
            };
            return crate::deploy::run(binary, &targets, is_group, remote_path.as_deref());
        }
        Command::Version { all, host } => {
            if *all {
                return crate::version::run_remote("all", true);
            }
            if let Some(h) = host {
                return crate::version::run_remote(h, false);
            }
            return crate::version::run_local();
        }
        Command::Plugin {
            action,
            target,
            content,
            name,
            force,
            json,
            lang,
            body,
            stdin,
            open,
        } => {
            return plugin::run(PluginCli {
                action,
                target: target.as_deref(),
                name: name.as_deref(),
                content: content.as_deref(),
                body: body.as_deref(),
                force: *force,
                json: *json,
                lang: lang.as_deref(),
                stdin: *stdin,
                open: *open,
            });
        }
        Command::Sign { exe, trust } => {
            return crate::sign::run(exe.as_deref(), *trust);
        }
        Command::Publish {
            repo,
            no_deploy,
            no_push,
            message,
        } => {
            return crate::publish::run(repo.as_deref(), *no_deploy, *no_push, message.as_deref());
        }
        Command::Upgrade {
            repo,
            check,
            features,
            no_build,
        } => {
            crate::upgrade::run(repo.as_deref(), *check, features.as_deref(), *no_build)?;
            let _ = plugin::install_tools_bin_from_sibling();
            let _ = plugin::seed_all(false);
            return Ok(());
        }
        Command::Info { json } => {
            return crate::info::run(*json);
        }
        Command::Exec {
            code,
            b64,
            lang,
            write,
            file,
            login,
            json,
            container,
            db,
            sql_user,
        } => {
            return run_exec(
                cli.host.as_deref(),
                cli.group.as_deref(),
                code.clone(),
                *b64,
                lang.clone(),
                write.clone(),
                file.clone(),
                *login,
                *json,
                container.clone(),
                db.clone(),
                sql_user.clone(),
            );
        }
        Command::External(args) => {
            let args = plugin::strip_global_flags(args);
            return plugin::run_external(
                &args,
                cli.host.as_deref().or(raw_host.as_deref()),
                cli.group.as_deref().or(raw_group.as_deref()),
            );
        }
    }
}

fn run_exec(
    host: Option<&str>,
    group: Option<&str>,
    code: Option<String>,
    b64: bool,
    lang: Option<String>,
    write: Option<PathBuf>,
    file: Option<PathBuf>,
    login: bool,
    json: bool,
    container: Option<String>,
    db: Option<String>,
    sql_user: Option<String>,
) -> anyhow::Result<()> {
    let go = |remote: Option<&crate::remote::RemoteChannel>| -> anyhow::Result<()> {
        let cs = if let Some(f) = &file {
            std::fs::read_to_string(f)?
        } else {
            code.clone().unwrap_or_default()
        };
        crate::exec::run(
            &cs,
            b64,
            lang.as_deref(),
            write.as_ref(),
            remote,
            login,
            json,
            container.as_deref(),
            db.as_deref(),
            sql_user.as_deref(),
        )?;
        Ok(())
    };
    if let Some(g) = group {
        let hosts = crate::hosts::HostsFile::load()?;
        for member in hosts.get_group_members(g)? {
            eprintln!("\n=== [{}] ===", member);
            let rc = crate::remote::RemoteChannel::connect(&member)?;
            go(Some(&rc))?;
        }
        return Ok(());
    }
    if let Some(h) = host {
        let rc = crate::remote::RemoteChannel::connect(h)?;
        return go(Some(&rc));
    }
    go(None)
}

fn describe_merged() -> anyhow::Result<()> {
    use clap::CommandFactory;
    let mut core = crate::describe::schema_from_command(Cli::command());
    if let Ok(tools) = plugin::tools_describe() {
        if let Some(core_cmds) = core.get_mut("commands").and_then(|c| c.as_array_mut()) {
            if let Some(extra) = tools.get("commands").and_then(|c| c.as_array()) {
                let installed: Vec<String> = plugin::installed_names();
                for cmd in extra {
                    let name = cmd.get("name").and_then(|n| n.as_str()).unwrap_or_default();
                    if CORE_COMMANDS.contains(&name) {
                        continue;
                    }
                    if installed.iter().any(|n| n == name) {
                        core_cmds.push(cmd.clone());
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&core)?);
    Ok(())
}

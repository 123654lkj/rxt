use std::path::Path;
use std::fs;

/// 补丁工具 — 生成/预览补丁
pub fn run(paths: &[String], reverse: bool, check: bool, output: Option<&str>) -> anyhow::Result<()> {
    if paths.is_empty() && !check {
        return diff_output(output);
    }

    if reverse {
        return apply_patch(paths.first().map(|s| s.as_str()), true, check);
    }

    if let Some(patch_file) = paths.first() {
        return apply_patch(Some(patch_file), false, check);
    }

    Ok(())
}

fn diff_output(output: Option<&str>) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("diff");
    if let Some(o) = output {
        let out_path = Path::new(o);
        let parent = out_path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let out = cmd.output().map_err(|e| anyhow::anyhow!("git diff failed: {}", e))?;
        fs::write(out_path, &out.stdout)?;
        println!("  wrote diff -> {}", out_path.display());
    } else {
        let out = cmd.output().map_err(|e| anyhow::anyhow!("git diff failed: {}", e))?;
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }
    Ok(())
}

fn apply_patch(patch_file: Option<&str>, reverse: bool, _check: bool) -> anyhow::Result<()> {
    let patch = patch_file.ok_or_else(|| anyhow::anyhow!("No patch file specified"))?;
    let patch_path = Path::new(patch);
    if !patch_path.exists() {
        anyhow::bail!("Patch file not found: {}", patch);
    }

    let mut cmd = std::process::Command::new("git");
    cmd.args(["apply", "--stat", "--apply", patch]);
    if reverse { cmd.arg("--reverse"); }
    if _check { cmd.arg("--check"); }
    let output = cmd.output().map_err(|e| anyhow::anyhow!("git apply failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() { println!("{}", stdout); }
    if !stderr.trim().is_empty() {
        if _check { println!("Patch check: {}", if output.status.success() { "OK" } else { "FAILED" }); }
        else { eprintln!("{}", stderr); }
    }
    if output.status.success() {
        println!("  applied patch: {}", patch);
    } else {
        anyhow::bail!("Failed to apply patch");
    }
    Ok(())
}

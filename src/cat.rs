use std::fs;
use std::io::Write;
use std::path::Path;

use crate::signature::to_utf8_lf;
use crate::signature::FileSignature;

pub fn run(path: &Path) -> anyhow::Result<()> {
    if let Ok(meta) = path.metadata() {
        let max = crate::common::max_text_bytes();
        if meta.len() > max {
            anyhow::bail!(
                "拒绝整读 {}（{} > {}）。避免 OOM；设 RXT_MAX_TEXT_BYTES 或改用 ffmpeg/hexdump",
                path.display(),
                crate::common::format_bytes(meta.len()),
                crate::common::format_bytes(max)
            );
        }
    }
    let raw = fs::read(path)?;
    let sig = FileSignature::detect(&raw);
    let text = to_utf8_lf(&raw, &sig);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(text.as_bytes())?;
    if !text.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    eprintln!(
        "  encoding: {} | line_ending: {} | bytes: {}",
        sig.encoding,
        sig.line_ending,
        raw.len()
    );
    Ok(())
}

use std::path::Path;
use std::fs;
use std::io::Write;

use crate::signature::to_utf8_lf;
use crate::signature::FileSignature;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let raw = fs::read(path)?;
    let sig = FileSignature::detect(&raw);
    let text = to_utf8_lf(&raw, &sig);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(text.as_bytes())?;
    if !text.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    eprintln!("  encoding: {} | line_ending: {} | bytes: {}", sig.encoding, sig.line_ending, raw.len());
    Ok(())
}

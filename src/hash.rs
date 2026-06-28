use std::path::Path;
use std::io::Read;

/// 文件哈希 — SHA256 / MD5
pub fn run(path: Option<&Path>, algo: &str, text: Option<&str>) -> anyhow::Result<()> {
    let data: Vec<u8> = if let Some(t) = text {
        t.as_bytes().to_vec()
    } else if let Some(p) = path {
        let mut f = std::fs::File::open(p)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        buf
    } else {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        buf
    };

    match algo {
        "sha256" | "sha-256" => {
            use sha2::Digest;
            let hash = sha2::Sha256::digest(&data);
            println!("{}", hex::encode(hash));
        }
        "md5" => {
            use md5::Digest;
            let hash = md5::Md5::digest(&data);
            println!("{}", hex::encode(hash));
        }
        _ => anyhow::bail!("Unsupported algorithm: {}. Use sha256 or md5", algo),
    }
    Ok(())
}

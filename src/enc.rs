/// Base64 / Hex / URL 编码解码
pub fn run(mode: &str, input: Option<&str>, decode: bool) -> anyhow::Result<()> {
    let data = if let Some(s) = input {
        s.as_bytes().to_vec()
    } else {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        buf
    };

    match mode {
        "base64" | "b64" => {
            if decode {
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(&String::from_utf8_lossy(&data).trim())?;
                println!("{}", String::from_utf8_lossy(&decoded));
            } else {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
                println!("{}", encoded);
            }
        }
        "hex" => {
            if decode {
                let decoded = hex::decode(String::from_utf8_lossy(&data).trim())?;
                println!("{}", String::from_utf8_lossy(&decoded));
            } else {
                println!("{}", hex::encode(&data));
            }
        }
        "url" => {
            if decode {
                let text = String::from_utf8_lossy(&data).into_owned();
                let decoded =
                    urlencoding::decode(&text).map_err(|e| anyhow::anyhow!("URL decode: {}", e))?;
                println!("{}", decoded);
            } else {
                let text = String::from_utf8_lossy(&data);
                let encoded = urlencoding::encode(&text);
                println!("{}", encoded);
            }
        }
        _ => anyhow::bail!("Unsupported mode: {}. Use base64, hex, or url", mode),
    }
    Ok(())
}

use std::io::Read;

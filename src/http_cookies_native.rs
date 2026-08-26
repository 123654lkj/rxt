//! 不依赖 rookiepy：Firefox sqlite 明文；Chromium 在 Windows 上 DPAPI 解 v10。
//! Chrome/Edge 127+ v20 App-Bound 不解值（不做 IElevator）。

use super::*;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn load(
    key: &str,
    domains: Option<Vec<String>>,
) -> anyhow::Result<(String, Vec<CookieRec>)> {
    match key {
        "firefox" | "librewolf" | "zen" => {
            let recs = load_firefox(key, domains.as_deref())?;
            Ok((key.to_string(), recs))
        }
        "chrome" | "edge" | "brave" | "chromium" | "opera" | "vivaldi" | "arc" | "opera-gx"
        | "operagx" => {
            let dir = chromium_user_data(key)
                .ok_or_else(|| anyhow::anyhow!("未找到 {key} User Data 目录"))?;
            let recs = load_chromium_user_data(&dir, domains)?;
            Ok((key.to_string(), recs))
        }
        _ => anyhow::bail!("native 不支持 {key}"),
    }
}

pub(super) fn load_chromium_user_data(
    user_data: &Path,
    domains: Option<Vec<String>>,
) -> anyhow::Result<Vec<CookieRec>> {
    let pairs = chromium_cookie_pairs(user_data);
    if pairs.is_empty() {
        anyhow::bail!(
            "不是 Chromium User Data（没找到 Cookies）: {}",
            user_data.display()
        );
    }
    let key = chromium_os_crypt_key(user_data);
    let mut recs = Vec::new();
    let mut v20 = 0usize;
    for (_ls, db) in &pairs {
        match read_chromium_db(db, key.as_deref(), &mut v20) {
            Ok(mut r) => recs.append(&mut r),
            Err(e) => eprintln!("# Cookie DB 跳过 {}: {e}", db.display()),
        }
    }
    if let Some(ds) = domains.as_ref() {
        recs.retain(|c| ds.iter().any(|d| domain_matches(&c.domain, d)));
    }
    if recs.is_empty() {
        anyhow::bail!("User Data 里 Cookie 为空: {}", user_data.display());
    }
    let empty = recs.iter().filter(|c| c.value.is_empty()).count();
    if v20 > 0 || empty == recs.len() {
        eprintln!(
            "# Chromium Cookie {} 条，其中 {} 条无值（v20 App-Bound 不解）。SSO 请 --browser firefox 或 --cookie-json",
            recs.len(),
            empty
        );
    }
    Ok(recs)
}

fn chromium_user_data(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        let roam = std::env::var("APPDATA").ok().unwrap_or_default();
        let p = match name {
            "edge" => PathBuf::from(&local).join("Microsoft/Edge/User Data"),
            "chrome" => PathBuf::from(&local).join("Google/Chrome/User Data"),
            "brave" => PathBuf::from(&local).join("BraveSoftware/Brave-Browser/User Data"),
            "vivaldi" => PathBuf::from(&local).join("Vivaldi/User Data"),
            "opera" => PathBuf::from(&roam).join("Opera Software/Opera Stable"),
            "opera-gx" | "operagx" => PathBuf::from(&roam).join("Opera Software/Opera GX Stable"),
            "arc" => PathBuf::from(&local).join(
                "Packages/TheBrowserCompany.Arc_ttt1ap7aakg6g/LocalCache/Local/Arc/User Data",
            ),
            "chromium" => PathBuf::from(&local).join("Chromium/User Data"),
            _ => return None,
        };
        p.is_dir().then_some(p)
    }
    #[cfg(not(windows))]
    {
        let home = dirs::home_dir()?;
        let p = match name {
            "chrome" => home.join(".config/google-chrome"),
            "edge" => home.join(".config/microsoft-edge"),
            "brave" => home.join(".config/BraveSoftware/Brave-Browser"),
            "chromium" => home.join(".config/chromium"),
            "vivaldi" => home.join(".config/vivaldi"),
            "opera" => home.join(".config/opera"),
            _ => return None,
        };
        p.is_dir().then_some(p)
    }
}

fn firefox_roots(kind: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(roam) = std::env::var("APPDATA") {
            let base = PathBuf::from(roam);
            let sub = match kind {
                "librewolf" => "librewolf",
                "zen" => "zen",
                _ => "Mozilla/Firefox",
            };
            v.push(base.join(sub));
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            v.push(match kind {
                "librewolf" => home.join(".librewolf"),
                "zen" => home.join(".zen"),
                _ => home.join(".mozilla/firefox"),
            });
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let app = home.join("Library/Application Support");
            v.push(match kind {
                "librewolf" => app.join("LibreWolf"),
                "zen" => app.join("zen"),
                _ => app.join("Firefox"),
            });
        }
    }
    let _ = kind;
    v
}

fn firefox_cookie_dbs(kind: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in firefox_roots(kind) {
        for dir in [root.join("Profiles"), root.clone()] {
            let Ok(rd) = fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path().join("cookies.sqlite");
                if p.is_file() {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn load_firefox(kind: &str, domains: Option<&[String]>) -> anyhow::Result<Vec<CookieRec>> {
    let dbs = firefox_cookie_dbs(kind);
    if dbs.is_empty() {
        anyhow::bail!("未找到 {kind} cookies.sqlite");
    }
    let mut recs = Vec::new();
    for db in &dbs {
        match read_firefox_db(db) {
            Ok(mut r) => recs.append(&mut r),
            Err(e) => eprintln!("# Firefox DB 跳过 {}: {e}", db.display()),
        }
    }
    if let Some(ds) = domains {
        recs.retain(|c| ds.iter().any(|d| domain_matches(&c.domain, d)));
    }
    if recs.is_empty() {
        anyhow::bail!("{kind} Cookie 为空");
    }
    Ok(recs)
}

fn copy_sqlite(src: &Path) -> anyhow::Result<PathBuf> {
    let tmp = std::env::temp_dir().join(format!(
        "rxt-ck-{}-{}.sqlite",
        std::process::id(),
        now_unix()
    ));
    fs::copy(src, &tmp)?;
    for suf in ["-wal", "-shm"] {
        let mut w = src.as_os_str().to_os_string();
        w.push(suf);
        let wp = PathBuf::from(w);
        if wp.is_file() {
            let mut d = tmp.as_os_str().to_os_string();
            d.push(suf);
            let _ = fs::copy(&wp, PathBuf::from(d));
        }
    }
    Ok(tmp)
}

fn cleanup_sqlite(tmp: &Path) {
    let _ = fs::remove_file(tmp);
    for suf in ["-wal", "-shm"] {
        let mut d = tmp.as_os_str().to_os_string();
        d.push(suf);
        let _ = fs::remove_file(PathBuf::from(d));
    }
}

fn read_firefox_db(src: &Path) -> anyhow::Result<Vec<CookieRec>> {
    let tmp = copy_sqlite(src)?;
    let recs = (|| -> anyhow::Result<Vec<CookieRec>> {
        let conn = Connection::open(&tmp)?;
        let mut stmt = conn.prepare(
            "SELECT host, path, isSecure, isHttpOnly, expiry, name, value FROM moz_cookies",
        )?;
        let mut out = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let host: String = row.get(0)?;
            let path: String = row.get(1).unwrap_or_else(|_| "/".into());
            let secure: i64 = row.get(2).unwrap_or(0);
            let http_only: i64 = row.get(3).unwrap_or(0);
            let expiry: i64 = row.get(4).unwrap_or(0);
            out.push(CookieRec {
                domain: host,
                path: if path.is_empty() { "/".into() } else { path },
                secure: secure != 0,
                http_only: http_only != 0,
                expires: (expiry > 0).then_some(expiry as u64),
                name: row.get(5)?,
                value: row.get(6).unwrap_or_default(),
            });
        }
        Ok(out)
    })();
    cleanup_sqlite(&tmp);
    recs
}

fn read_chromium_db(
    src: &Path,
    key: Option<&[u8]>,
    v20: &mut usize,
) -> anyhow::Result<Vec<CookieRec>> {
    let tmp = copy_sqlite(src)?;
    let recs = (|| -> anyhow::Result<Vec<CookieRec>> {
        let conn = Connection::open(&tmp)?;
        let mut stmt = conn.prepare(
            "SELECT host_key, path, is_secure, is_httponly, expires_utc, name, value, encrypted_value FROM cookies",
        )?;
        let mut out = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let host: String = row.get(0)?;
            let path: String = row.get(1).unwrap_or_else(|_| "/".into());
            let secure: i64 = row.get(2).unwrap_or(0);
            let http_only: i64 = row.get(3).unwrap_or(0);
            let expires_utc: i64 = row.get(4).unwrap_or(0);
            let name: String = row.get(5)?;
            let plain: String = row.get(6).unwrap_or_default();
            let enc: Vec<u8> = row.get(7).unwrap_or_default();
            let value = if !plain.is_empty() {
                plain
            } else {
                match decrypt_chromium_value(&enc, key, v20) {
                    Some(s) => s,
                    None => String::new(),
                }
            };
            let expires = if expires_utc > 0 {
                let unix = expires_utc / 1_000_000 - 11_644_473_600;
                (unix > 0).then_some(unix as u64)
            } else {
                None
            };
            out.push(CookieRec {
                domain: host,
                path: if path.is_empty() { "/".into() } else { path },
                secure: secure != 0,
                http_only: http_only != 0,
                expires,
                name,
                value,
            });
        }
        Ok(out)
    })();
    cleanup_sqlite(&tmp);
    recs
}

fn chromium_os_crypt_key(user_data: &Path) -> Option<Vec<u8>> {
    let raw = fs::read_to_string(user_data.join("Local State")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let b64 = v
        .get("os_crypt")
        .and_then(|o| o.get("encrypted_key"))
        .and_then(|x| x.as_str())?;
    use base64::Engine as _;
    let mut bin = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    const DPAPI: &[u8] = b"DPAPI";
    if bin.starts_with(DPAPI) {
        bin.drain(..DPAPI.len());
    }
    dpapi_unprotect(&bin)
}

fn decrypt_chromium_value(enc: &[u8], key: Option<&[u8]>, v20: &mut usize) -> Option<String> {
    if enc.is_empty() {
        return None;
    }
    if enc.starts_with(b"v20") {
        *v20 += 1;
        return None;
    }
    if enc.starts_with(b"v10") || enc.starts_with(b"v11") {
        let key = key?;
        if enc.len() < 3 + 12 + 16 {
            return None;
        }
        let nonce = &enc[3..15];
        let ct = &enc[15..];
        return aes_gcm_decrypt(key, nonce, ct);
    }
    if let Some(b) = dpapi_unprotect(enc) {
        return Some(String::from_utf8_lossy(&b).into_owned());
    }
    None
}

fn aes_gcm_decrypt(key: &[u8], nonce: &[u8], ct: &[u8]) -> Option<String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    if key.len() != 32 || nonce.len() != 12 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let n = Nonce::from_slice(nonce);
    let pt = cipher.decrypt(n, ct).ok()?;
    String::from_utf8(pt).ok()
}

fn dpapi_unprotect(data: &[u8]) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi_unprotect_win(data)
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        None
    }
}

#[cfg(windows)]
fn dpapi_unprotect_win(data: &[u8]) -> Option<Vec<u8>> {
    #[repr(C)]
    struct Blob {
        cb_data: u32,
        pb_data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptUnprotectData(
            data_in: *mut Blob,
            descr: *mut *mut u16,
            entropy: *mut Blob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }
    if data.is_empty() {
        return None;
    }
    let mut input = Blob {
        cb_data: data.len() as u32,
        pb_data: data.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0x1, // CRYPTPROTECT_UI_FORBIDDEN
            &mut output,
        )
    };
    if ok == 0 || output.pb_data.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) };
    let v = slice.to_vec();
    unsafe {
        LocalFree(output.pb_data as *mut core::ffi::c_void);
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_sqlite_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "rxt-ff-{}-{}.sqlite",
            std::process::id(),
            now_unix()
        ));
        let conn = Connection::open(&tmp).unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_cookies (host TEXT, path TEXT, isSecure INTEGER, isHttpOnly INTEGER, expiry INTEGER, name TEXT, value TEXT);
             INSERT INTO moz_cookies VALUES ('.example.com','/',1,0,2000000000,'sid','secret-ff');",
        )
        .unwrap();
        drop(conn);
        let recs = read_firefox_db(&tmp).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].name, "sid");
        assert_eq!(recs[0].value, "secret-ff");
        assert_eq!(recs[0].domain, ".example.com");
        let _ = std::fs::remove_file(&tmp);
    }
}

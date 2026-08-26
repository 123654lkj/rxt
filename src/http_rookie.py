# rxt http --browser 的磁盘读 Cookie（rookiepy）。stdout 仅 JSON。
import json
import os
import sys
from pathlib import Path

import rookiepy

name = os.environ.get("RXT_BROWSER_NAME", "chrome").strip().lower().replace("_", "-")
raw = os.environ.get("RXT_COOKIE_DOMAINS", "").strip()
domains = [d for d in raw.split(",") if d] or None
chromium_dir = os.environ.get("RXT_CHROMIUM_DIR", "").strip()

FNS = {}
for n in (
    "chrome",
    "edge",
    "firefox",
    "brave",
    "chromium",
    "opera",
    "vivaldi",
    "arc",
    "zen",
    "librewolf",
    "safari",
    "internet_explorer",
    "cachy",
    "octo_browser",
):
    fn = getattr(rookiepy, n, None)
    if fn:
        FNS[n.replace("_", "-")] = fn
if "librewolf" in FNS:
    FNS["libre-wolf"] = FNS["librewolf"]
if "octo-browser" in FNS:
    FNS["octo"] = FNS["octo-browser"]
gx = getattr(rookiepy, "opera_gx", None)
if gx:
    FNS["opera-gx"] = gx
    FNS["operagx"] = gx

ALL = [
    "chrome",
    "edge",
    "firefox",
    "brave",
    "chromium",
    "opera",
    "vivaldi",
    "arc",
    "zen",
    "librewolf",
    "opera-gx",
    "tabbit",
]


def dump(cookies):
    out = []
    for c in cookies:
        if not isinstance(c, dict):
            c = {
                "domain": getattr(c, "domain", ""),
                "path": getattr(c, "path", "/"),
                "secure": bool(getattr(c, "secure", False)),
                "http_only": bool(getattr(c, "http_only", False)),
                "name": getattr(c, "name", ""),
                "value": getattr(c, "value", ""),
            }
        out.append(
            {
                "domain": c.get("domain") or "",
                "path": c.get("path") or "/",
                "secure": bool(c.get("secure")),
                "httpOnly": bool(c.get("http_only", c.get("httponly", c.get("httpOnly", False)))),
                "name": c.get("name") or "",
                "value": c.get("value") or "",
            }
        )
    json.dump(out, sys.stdout, ensure_ascii=False)


def chromium_pairs(user_data: Path):
    if not user_data.is_dir():
        return []
    local_state = user_data / "Local State"
    profiles = [user_data / "Default"]
    try:
        for p in user_data.iterdir():
            if p.is_dir() and p.name.startswith("Profile "):
                profiles.append(p)
    except OSError:
        pass
    out = []
    for p in profiles:
        for c in (p / "Network" / "Cookies", p / "Cookies"):
            if c.is_file():
                out.append((local_state, c))
    return out


def tabbit_roots():
    home = Path.home()
    roots = []
    local = os.environ.get("LOCALAPPDATA", "")
    roam = os.environ.get("APPDATA", "")
    if local:
        for n in ("Tabbit", "Tabbit Browser", "tabbit", "TabbitAI"):
            roots.append(Path(local) / n / "User Data")
    if roam:
        for n in ("Tabbit", "Tabbit Browser", "tabbit"):
            roots.append(Path(roam) / n / "User Data")
    mac = home / "Library" / "Application Support"
    for n in ("Tabbit", "Tabbit Browser", "tabbit"):
        roots.append(mac / n)
    xdg = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config"))
    for n in ("tabbit", "Tabbit", "tabbit-browser", "Tabbit Browser"):
        roots.append(xdg / n)
    return roots


def from_chromium_dir(user_data: Path):
    any_fn = getattr(rookiepy, "any_browser", None)
    based = getattr(rookiepy, "chromium_based", None)
    cookies = []
    last = None
    for local_state, db in chromium_pairs(user_data):
        try:
            if based:
                try:
                    cookies.extend(based(str(local_state), str(db), domains) or [])
                    continue
                except TypeError:
                    pass
            if any_fn:
                cookies.extend(any_fn(str(db), domains, str(local_state) if local_state.is_file() else None) or [])
        except Exception as e:
            last = e
    if not cookies and last:
        raise last
    return cookies


def load_named(n):
    if n in ("tabbit", "tabbit-browser"):
        last = "未找到 Tabbit User Data"
        for root in tabbit_roots():
            if not root.is_dir():
                continue
            try:
                c = from_chromium_dir(root)
            except Exception as e:
                last = f"{root}: {e}"
                continue
            if c:
                return c
        raise RuntimeError(f"Tabbit Cookie 读失败。{last}")
    fn = FNS.get(n)
    if fn is None:
        raise RuntimeError(f"未知浏览器: {n}")
    return fn(domains)


if chromium_dir:
    dump(from_chromium_dir(Path(chromium_dir)))
    sys.exit(0)

if name == "auto":
    last = "无候选"
    for cand in ALL:
        try:
            cookies = load_named(cand)
        except Exception as e:
            last = f"{cand}: {e}"
            continue
        if cookies:
            dump(cookies)
            sys.exit(0)
    sys.stderr.write(f"auto 未读到 Cookie。{last}\n")
    sys.exit(2)

if name == "all":
    merged = []
    for cand in ALL:
        try:
            merged.extend(load_named(cand) or [])
        except Exception:
            continue
    dump(merged)
    sys.exit(0)

try:
    dump(load_named(name) or [])
except Exception as e:
    sys.stderr.write(str(e) + "\n")
    sys.exit(2)

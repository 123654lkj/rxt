# rxt http --browser 的磁盘读 Cookie（rookiepy）。stdout 仅 JSON。
import json
import os
import sys

import rookiepy

name = os.environ.get("RXT_BROWSER_NAME", "chrome").strip().lower()
raw = os.environ.get("RXT_COOKIE_DOMAINS", "").strip()
domains = [d for d in raw.split(",") if d] or None
fns = {
    "chrome": rookiepy.chrome,
    "edge": rookiepy.edge,
    "firefox": rookiepy.firefox,
    "brave": rookiepy.brave,
    "chromium": rookiepy.chromium,
}


def dump(cookies):
    out = []
    for c in cookies:
        out.append(
            {
                "domain": c.get("domain") or "",
                "path": c.get("path") or "/",
                "secure": bool(c.get("secure")),
                "httpOnly": bool(c.get("http_only", c.get("httponly", False))),
                "name": c.get("name") or "",
                "value": c.get("value") or "",
            }
        )
    json.dump(out, sys.stdout, ensure_ascii=False)


if name == "auto":
    last = "无候选"
    for cand in ("chrome", "edge", "firefox", "brave"):
        try:
            cookies = fns[cand](domains)
        except Exception as e:
            last = f"{cand}: {e}"
            continue
        if cookies:
            dump(cookies)
            sys.exit(0)
    sys.stderr.write(f"auto 未读到 Cookie。{last}\n")
    sys.exit(2)

fn = fns.get(name)
if fn is None:
    sys.stderr.write(f"未知浏览器: {name}\n")
    sys.exit(2)
dump(fn(domains))

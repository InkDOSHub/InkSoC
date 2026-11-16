#!/usr/bin/env python3
"""Small static smoke test for the InkDOS Breeze shell app catalog."""
from pathlib import Path
import posixpath
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
desktop = (ROOT / "desktop.js").read_text(encoding="utf-8")
catalog = re.findall(r"path:'([^']+)'", desktop)
if not catalog:
    raise SystemExit("No WEB_APPS paths found in desktop.js")

forbidden = ("inkmind", "inkarcade", "wordcraft", "gridcraft", "deckcraft")
catalog_text = desktop[desktop.find("const WEB_APPS"):desktop.find("];", desktop.find("const WEB_APPS"))]
if any(token in catalog_text.lower() for token in forbidden):
    raise SystemExit("An excluded game/AI/ArtCraft app appears in the shell catalog")

errors = []
for entry in catalog:
    entry_path = ROOT / entry
    if not entry_path.is_file():
        errors.append(f"catalog entry missing: {entry}")
        continue
    if entry.endswith("platform/docs/index.html"):
        html_path = entry
    else:
        html_path = entry
    html = entry_path.read_text(encoding="utf-8")
    if "shell-bridge.js" not in html:
        errors.append(f"shell bridge missing: {entry}")
    for raw in re.findall(r'(?:src|href)=[\\"\']([^\\"\']+)[\\"\']', html):
        parts = urlsplit(raw)
        if parts.scheme or parts.netloc or raw.startswith("#") or parts.path in ("", "/"):
            continue
        target = unquote(parts.path)
        if target.startswith("/"):
            resolved = posixpath.normpath(target.lstrip("/"))
        else:
            resolved = posixpath.normpath(posixpath.join(posixpath.dirname(html_path), target))
        if resolved.startswith("../") or not (ROOT / resolved).is_file():
            errors.append(f"{entry}: unresolved local reference {raw} -> {resolved}")

docs = ROOT / "platform/docs/index.html"
if not docs.is_file() or 'data-inkdos-open="pdf"' not in docs.read_text(encoding="utf-8"):
    errors.append("internal documentation is missing or cannot launch the PDF app")

if errors:
    print("\n".join(errors), file=sys.stderr)
    raise SystemExit(1)
print(f"OK: {len(catalog)} shell apps, local entrypoints and shell bridge verified")

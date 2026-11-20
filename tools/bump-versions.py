"""Raises the cache-busting ?v= numbers after a new engine build, so browsers load
the new inksoc.wasm / inksoc-mt.wasm and the scripts that name them."""
import re


def bump(path, names):
    text = open(path).read()
    for name in names:
        text = re.sub(re.escape(name) + r"\?v=(\d+)", lambda m: f"{name}?v={int(m.group(1)) + 1}", text)
    open(path, "w").write(text)


bump("worker.js", ["inksoc.wasm", "inksoc-mt.wasm"])
bump("engine.js", ["worker.js"])
bump("index.html", ["engine.js"])

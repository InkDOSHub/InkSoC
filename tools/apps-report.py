"""Builds compat/apps-results.md from the <name>-result.json files of a compat-apps run:
one line per program, the most common stop reasons first, so a fix can aim at what
blocks the most programs."""
import glob
import json
import os
import sys
from collections import Counter

folder, commit = sys.argv[1], sys.argv[2][:7]
apps = {a["name"]: a for a in json.load(open("compat/apps.json"))}
rows = {}
for f in glob.glob(os.path.join(folder, "**", "*-result.json"), recursive=True):
    r = json.load(open(f))
    rows[r["name"]] = r


def verdict(r):
    if r is None:
        return "not run", ""
    s, err = r["status"], (r.get("error") or "").split("\n")[0]
    if s == "ok" and not r.get("exit"):
        return "finished", ""
    if s == "ok":
        return "exit code %s" % r.get("exit"), err
    if s == "timeout":
        return ("running, %s window(s)" % r.get("wins")) if r.get("wins") else "running, no window", ""
    return s, err


lines = ["# Common programs on InkSoC", "", "Engine %s. Each program is downloaded from its official site and its installer (or the program itself) runs on the engine in Node on GitHub Actions (.github/workflows/compat-apps.yml). \"running\" = still going at the time limit." % commit, "",
         "| Program | Group | Result | Where it stopped |", "|---|---|---|---|"]
reasons = Counter()
for name, a in apps.items():
    v, err = verdict(rows.get(name))
    lines.append("| %s | %s | %s | %s |" % (name, a.get("group", ""), v, err.replace("|", "/")[:160]))
    if err:
        reasons[err.split("(rip")[0].strip()[:80]] += 1
    for m in (rows.get(name) or {}).get("missing", []):
        reasons["missing: " + m] += 1
lines += ["", "## Most common stops", ""] + ["- %d× %s" % (n, r) for r, n in reasons.most_common(15)]
open("compat/apps-results.md", "w").write("\n".join(lines) + "\n")
print("\n".join(lines))

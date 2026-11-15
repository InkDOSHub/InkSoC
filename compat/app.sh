#!/bin/bash
# app.sh <folder> <exe> [args]: runs one Windows program on the engine in Node with every
# Wine DLL (64- and 32-bit) and the files of <folder> next to it. Environment as in run.mjs:
# LIMIT (seconds), SHOT (screen .ppm, rewritten every 30 s), INPUT (clicks), TRACE, TRACE_AT.
cd "$(dirname "$0")/.."
J=$(python3 - "$1" "$2" "$3" <<'P'
import os, json, sys
d, exe, args = sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else ''
fs = [os.path.join(d, f) for f in sorted(os.listdir(d))]
# what the page loads for every program (bin/wine/list.json), plus the 32-bit DLLs
fs += [os.path.join('bin/wine', f) for f in json.load(open('bin/wine/list.json'))]
fs += [os.path.join('bin/wine32', f) for f in sorted(os.listdir('bin/wine32')) if f.endswith(('.dll', '.nls'))]
print(json.dumps({'files': fs, 'exe': exe, 'args': args}))
P
)
node ${PROF:+--cpu-prof --cpu-prof-dir="$PROF"} compat/run.mjs --one "$J"

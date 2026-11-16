#!/bin/sh
# Moves the Linux engine to a newer Windows engine, without touching the Windows engine:
#   tools/sync-engine.sh <commit or tag of the Windows engine>   (run from the repository root)
# 1. takes the engine (src/engine of InkDOSHub/InkSoC) at that commit, 2. re-applies linux.patch (the Linux-only
# changes), 3. builds, 4. runs tests/linux-test.mjs. Only if all of it passes does it replace
# engine-src, engine.wasm and engine-base.txt. If the patch no longer fits, nothing changes and
# the conflicts are listed: fix them in the new tree, then run make-patch.sh.
set -e
# INKSOC: a clone of the InkSoC engine repository (default ../InkSoC next to this repo);
# ENGINE: the engine folder inside it (default src/engine; apps/inksoc/src/engine in the old Hub repo).
export INKSOC=${INKSOC:-$(cd .. && pwd)/InkSoC}
export ENGINE=${ENGINE:-src/engine}
ref=${1:?usage: tools/sync-engine.sh <commit or tag>}
new=$(git -C "$INKSOC" rev-parse --short "$ref^{commit}")
tmp=$(mktemp -d)
git -C "$INKSOC" archive "$new" "$ENGINE" | tar -x --strip-components=$(($(echo "$ENGINE" | tr -cd / | wc -c) + 1)) -C "$tmp"
echo "== applying linux.patch on the Windows engine $new"
if ! (cd "$tmp" && patch -p1 -F0 --no-backup-if-mismatch < "$OLDPWD/linux.patch"); then
  echo "The Linux changes do not fit this engine any more: fix the .rej files in $tmp"; exit 1
fi
echo "== building"
(cd "$tmp" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/alpine-engine-target}" cargo build --release --target wasm32-unknown-unknown 2>&1 | tail -1)
wasm="${CARGO_TARGET_DIR:-/tmp/alpine-engine-target}/wasm32-unknown-unknown/release/inksoc.wasm"
echo "== Linux test"
node tests/linux-test.mjs "$wasm" || { echo "Linux test failed: engine-src and engine.wasm stay as they were"; exit 1; }
rm -rf engine-src.old && mv engine-src engine-src.old && mkdir engine-src
cp -r "$tmp"/. engine-src/
cp engine-src.old/.gitignore engine-src/ 2>/dev/null || true
rm -rf engine-src.old "$tmp"
cp "$wasm" engine.wasm
echo "$new" > engine-base.txt
sh tools/make-patch.sh
echo "Done: the Linux engine now follows the Windows engine $new. Write its version (VERSIONS.md in the InkSoC repo) in engine-version.txt and bump engine.wasm?v= where it is loaded."

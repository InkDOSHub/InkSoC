#!/bin/sh
# Rebuilds linux.patch: what engine-src (this app's engine) changes on top of the Windows
# engine it is based on (engine-base.txt, a commit of the InkSoC engine). Run from the repository root.
set -e
# INKSOC: a clone of the InkSoC engine repository (default ../InkSoC next to this repo);
# ENGINE: the engine folder inside it (default src/engine; apps/inksoc/src/engine in the old Hub repo).
export INKSOC=${INKSOC:-$(cd .. && pwd)/InkSoC}
export ENGINE=${ENGINE:-src/engine}
base=$(cat engine-base.txt)
tmp=$(mktemp -d)
mkdir -p "$tmp/a"
git -C "$INKSOC" archive "$base" "$ENGINE" | tar -x --strip-components=$(($(echo "$ENGINE" | tr -cd / | wc -c) + 1)) -C "$tmp/a"
ln -s "$(pwd)/engine-src" "$tmp/b"
(cd "$tmp" && diff -ruN -x target -x .gitignore -x Cargo.lock a b) > linux.patch || true
rm -rf "$tmp"
echo "linux.patch: $(grep -c '^+++ ' linux.patch) files, $(wc -l < linux.patch) lines (base $base)"

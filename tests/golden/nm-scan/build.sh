#!/usr/bin/env bash
# Build the three probe staticlibs and stage them where `trantor scan` looks
# (target/trantor/nm-scan/platform/targets/arm64mac/lib<name>.a). No roc: these archives are never
# linked into an app, they only feed the symbol scan.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
mkdir -p target/trantor/nm-scan/platform/targets/arm64mac
for c in probe-a probe-b probe-c; do
  a="lib$(echo "$c" | tr - _).a"
  cp "target/release/$a" "target/trantor/nm-scan/platform/targets/arm64mac/$a"
done
echo "staged probe archives"

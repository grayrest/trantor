#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
ROC="${ROC:-$HOME/.bin/roc}"
GLUE="${GLUE:-$HOME/.bin/RustGlue.roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }

mkdir -p glue-out
cap "$ROC" glue "$GLUE" glue-out platform/main.roc
cp glue-out/roc_platform_abi.rs abi/src/generated.rs

cargo build --release
mkdir -p platform/targets/arm64mac
# stage every component staticlib the workspace produced
for a in target/release/lib*.a; do
  base=$(basename "$a")
  # skip the abi rlib-only crate (it has no staticlib); copy component archives
  cp "$a" "platform/targets/arm64mac/$base"
done

cap "$ROC" check app/main.roc
mkdir -p bin
cap "$ROC" build --output=bin/b0 app/main.roc
echo "built bin/b0"

#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
ROC="${ROC:-$HOME/.bin/roc}"; GLUE="${GLUE:-$HOME/.bin/RustGlue.roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }
mkdir -p glue-out
cap "$ROC" glue "$GLUE" glue-out platform/main.roc
cp glue-out/roc_platform_abi.rs abi/src/generated.rs
cargo build --release
mkdir -p platform/targets/arm64mac
for a in target/release/lib*.a; do cp "$a" "platform/targets/arm64mac/$(basename "$a")"; done
cap "$ROC" check app/main.roc
mkdir -p bin
cap "$ROC" build --output=bin/imview app/main.roc
echo built

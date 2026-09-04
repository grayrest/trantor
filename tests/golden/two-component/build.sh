#!/usr/bin/env bash
# The manual composition pipeline hematite will automate (H3). Running this
# reproduces bin/reader from the checked-in sources. Order matters.
set -euo pipefail
cd "$(dirname "$0")"
ROC="${ROC:-$HOME/.bin/roc}"
GLUE="${GLUE:-$HOME/.bin/RustGlue.roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }

# 1. Copy each pure-Roc component's modules into the platform (D13: verbatim).
cp components/pathlib/Path.roc platform/Path.roc

# 2. Generate the one ABI crate from the composed platform (D4).
mkdir -p glue-out
cap "$ROC" glue "$GLUE" glue-out platform/main.roc
cp glue-out/roc_platform_abi.rs abi/src/generated.rs

# 3. Build every component host into its archive; stage into targets/.
cargo build --release
mkdir -p platform/targets/arm64mac
for c in stdio capstdfs audit env cli; do
  cp "target/release/lib${c}.a" "platform/targets/arm64mac/lib${c}.a"
done

# 4. roc check BEFORE roc build (H1b: a mismatch segfaults build but checks clean).
cap "$ROC" check app/main.roc

# 5. Link the app against the composed platform.
mkdir -p bin
cap "$ROC" build --output=bin/reader app/main.roc
echo "built bin/reader"

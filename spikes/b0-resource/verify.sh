#!/usr/bin/env bash
# B0 acceptance: the resource model drop-balances. 3 opens -> 3 closes via the
# driver's roc_dealloc registry hook; a borrow does not close; exit code == closes.
set -euo pipefail
cd "$(dirname "$0")/../.."
S=spikes/b0-resource
cargo build --release -q
./target/release/hematite compose "$S" >/dev/null
( cd "$S" && ./build.sh >/dev/null 2>&1 )
set +e
err=$("$S/bin/b0" 2>&1 >/dev/null); code=$?
set -e
[[ $code -eq 3 ]] || { echo "FAIL: exit code $code (expected 3 == closes)"; echo "$err"; exit 1; }
grep -q "opens=3 closes=3 live=0" <<<"$err" || { echo "FAIL: gauge did not balance"; echo "$err"; exit 1; }
grep -q "close counter#2 (bumped 2 times)" <<<"$err" || { echo "FAIL: borrow closed early or bump after borrow lost"; echo "$err"; exit 1; }
echo "ok: 3 opens, 3 closes, live=0; borrow held; exit code == closes"
echo "B0 PASS"

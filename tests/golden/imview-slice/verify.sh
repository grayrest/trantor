#!/usr/bin/env bash
# H7 slice: the [Model : model]-parameterized reactor driver (roc-solid
# platform-im's hardest contract shape) composes, typechecks, and RUNS.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/imview-slice
cargo build --release -q
./target/release/hematite compose "$FIX" >/dev/null
grep -q "imview driver HOST" "$FIX/components/imview/src/lib.rs" || { echo "FAIL: authored host clobbered"; exit 1; }
if ! _b=$(./target/release/hematite build "$FIX" --app app --out imview 2>&1); then echo "FAIL: build imview" >&2; echo "$_b" >&2; exit 1; fi
out=$("$FIX/target/hematite/imview-slice/bin/imview")
[[ "$out" == "[hi | width-derived]" ]] || { echo "FAIL: got '$out'"; exit 1; }
echo "ok: [Model : model] reactor driver composes + runs — '$out'"
echo "H7 (platform-im contract slice) PASS"

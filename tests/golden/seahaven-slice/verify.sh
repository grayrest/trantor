#!/usr/bin/env bash
# H5 substitution proof: seahaven's REAL Stdout/Stderr derived layer runs over a
# hematite-composed stdio interface, and two interchangeable host impls (std vs
# capture) produce different output from the SAME unmodified derived layer.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/seahaven-slice
cargo build --release -q

# The derived layer is seahaven's real Stdout.roc (modulo the one-line import rename).
if ! diff -q \
   <(sed 's/import Stdio/import Host/; s/Stdio\.stdout/Host.stdout/g; s/Stdio\.stderr/Host.stderr/g' "$FIX/components/stdio-lib/Stdout.roc") \
   /Users/grayrest/dev/roc/seahaven/platform/Stdout.roc >/dev/null; then
  echo "FAIL: stdio-lib/Stdout.roc diverged from seahaven upstream"; exit 1
fi
echo "ok: derived layer == seahaven upstream Stdout.roc"

run_world() { # $1 world file -> stdout
  ./target/release/hematite compose "$FIX" --world "$1" >/dev/null
  ( cd "$FIX" && ./build.sh >/dev/null 2>&1 )
  if ! _sc=$(./target/release/hematite scan "$FIX" --world "$1" 2>&1); then echo "FAIL: nm-scan [$1]" >&2; echo "$_sc" >&2; exit 1; fi
  "$FIX/bin/reader" 2>/dev/null
}
std=$(run_world world.toml | head -1)
cap=$(run_world world-capture.toml | head -1)
[[ "$std" == "out: hello from a composed seahaven slice" ]] || { echo "FAIL std: '$std'"; exit 1; }
[[ "$cap" == "[cap] out: hello from a composed seahaven slice" ]] || { echo "FAIL cap: '$cap'"; exit 1; }
# restore std world as the committed default
./target/release/hematite compose "$FIX" >/dev/null
echo "ok: same derived layer, std impl -> '$std'"
echo "ok: same derived layer, capture impl -> '$cap'"
echo "H5 (substitution slice) PASS"

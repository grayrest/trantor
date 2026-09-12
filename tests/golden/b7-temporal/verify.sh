#!/usr/bin/env bash
# B7 acceptance: the trantor-temporal PACKAGE, consumed by a world that is not
# trantor-cli.
#
# This fixture used to carry its own copy of the interface and the host. The
# copy drifted — after the package's release rework it still held, and still
# asserted as correct, every defect that rework removed — so it consumes the
# package now and there is one source of truth again.
#
# What it proves that the package's own gate cannot: the package composes
# against a DIFFERENT driver with no basic-cli shim; temporal_rs is vendored by
# temporal-host ONLY (H0c), no other archive in the world carrying its symbols;
# a world without the package links none of it; and every resource the package
# hands out is dropped. The drop gauge is this WORLD's component, not the
# package's — counting live handles is a test's business, and `live!` has no
# place in everyone's API.
#
# Behaviour checks are deliberately thin here. The package's own verify.sh pins
# sixty; duplicating them is how the copy drifted in the first place.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b7-temporal
cargo build --release -q

if ! _b=$(./target/release/trantor build "$B" --app app --out b7 2>&1); then echo "FAIL: build b7" >&2; echo "$_b" >&2; exit 1; fi
set +e; out=$(cd "$B" && ./target/trantor/b7-temporal/bin/b7 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: exit $code (2-9 = a step failed; >=10 = leaked handles)"; echo "$out"; exit 1; }
want=$'date-add: 2024-02-29\nzdt-ny: 2024-03-10T01:30:00-05:00[America/New_York]\nzdt-tokyo: 2024-03-10T15:30:00+09:00[Asia/Tokyo]\nsame-instant: yes\nday-of-week: 4'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: the package composes into a non-trantor-cli world; NY/Tokyo conversion; live=0"

# H0c: temporal_rs natives live in libtemporal_host.a and nowhere else.
T="$B/target/trantor/b7-temporal/platform/targets/arm64mac"
ar t "$T/libtemporal_host.a" | grep "^temporal_rs-" >/dev/null || { echo "FAIL: libtemporal_host.a has no temporal_rs symbols"; exit 1; }
for a in "$T"/lib*.a; do
  [[ "$(basename "$a")" == "libtemporal_host.a" ]] && continue
  if ar t "$a" | grep "^temporal_rs-" >/dev/null; then echo "FAIL: $(basename "$a") also carries temporal_rs"; exit 1; fi
done
[[ -x tests/golden/b4-small/target/trantor/b4-small/bin/b4 ]] || ./target/release/trantor build tests/golden/b4-small --app app --out b4 >/dev/null 2>&1
# A stripped binary would make the negative grep pass vacuously; assert the
# symbol table is actually present first, then that it carries no temporal_rs.
b4syms=$(nm tests/golden/b4-small/target/trantor/b4-small/bin/b4 2>/dev/null | grep -c .)
[[ "$b4syms" -gt 100 ]] || { echo "FAIL: b4 has $b4syms symbols (stripped?) — the temporal_rs check would be vacuous"; exit 1; }
if nm tests/golden/b4-small/target/trantor/b4-small/bin/b4 2>/dev/null | grep temporal_rs >/dev/null; then echo "FAIL: temporal-less world (b4) links temporal_rs"; exit 1; fi
echo "ok: temporal_rs vendored by the package temporal-host only; a world without it links none"
echo "B7 PASS"

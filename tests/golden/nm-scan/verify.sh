#!/usr/bin/env bash
# H0c archive symbol-collision scan (the "nm-scan"): the compose-time guard that
# rejects the same global symbol defined by two components. Two vendored natives
# (two sqlites) linked into one world resolve silently by archive order
# (first-in wins) with no diagnostic — memory-unsafe if the copies differ. The
# linker will not catch it, so trantor does. See notes/2026-09-04-h0-link-shape.md.
#
# Proven here on constructed probe archives (a real sole-vendor world can't
# collide, so the danger case is manufactured):
#   (1) REJECT — probe-a and probe-b both export `trantor_probe_collision`; the
#                scan fails and names BOTH the symbol and the two components.
#   (2) HATCH  — declaring it in [world].shared_symbols (glob) passes.
#   (3) CLEAN  — {probe-a, probe-c} share no unmangled global; the scan passes.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/nm-scan
HEM=./target/release/trantor
cargo build --release -q
( cd "$FIX" && ./build.sh >/dev/null 2>&1 ) || { echo "FAIL: build probe archives"; exit 1; }

# (1) REJECT — the manufactured collision is caught, with a useful diagnostic.
set +e; out=$($HEM scan "$FIX" 2>&1); code=$?; set -e
[[ $code -ne 0 ]] || { echo "FAIL: scan passed a real collision"; echo "$out"; exit 1; }
grep -q 'trantor_probe_collision' <<<"$out" || { echo "FAIL: diagnostic omits the symbol"; echo "$out"; exit 1; }
{ grep -q 'probe-a' <<<"$out" && grep -q 'probe-b' <<<"$out"; } || { echo "FAIL: diagnostic omits a colliding component"; echo "$out"; exit 1; }
# ...and ONLY the planted symbol — no compiler-runtime / Rust-ODR false positives.
n=$(grep -c '— defined by' <<<"$out")
[[ "$n" -eq 1 ]] || { echo "FAIL: expected exactly 1 collision, got $n (a false positive slipped through)"; echo "$out"; exit 1; }
echo "ok: reject — exactly the planted symbol, naming both components"

# (2) HATCH — an explicit shared-native declaration exempts it.
set +e; sout=$($HEM scan "$FIX" --world world-shared.toml 2>&1); scode=$?; set -e
[[ $scode -eq 0 ]] || { echo "FAIL: shared_symbols glob did not exempt the collision"; echo "$sout"; exit 1; }
echo "ok: hatch — [world].shared_symbols glob exempts the declared native"

# (3) CLEAN — distinct components are not flagged.
set +e; cout=$($HEM scan "$FIX" --world world-solo.toml 2>&1); ccode=$?; set -e
[[ $ccode -eq 0 ]] || { echo "FAIL: clean world flagged"; echo "$cout"; exit 1; }
echo "ok: clean — no shared unmangled global, scan passes"

echo "NM-SCAN PASS"

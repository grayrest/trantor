#!/usr/bin/env bash
# P0 spike (plan 2026-09-09 H7, D-H7-5/7/8/11): a reactor driver + two service
# components with per-component unions spliced into the world's Cmd/Event/Env.
# Measures: (1) the nested-union wrapper crossing OUT (List(Cmd)) and IN
# (Event) of Roc; (2) HostCtx.wake from a worker thread -> runtime-thread
# complete; (3) the env block; (4) the gate hook chain; (5) the allocator-shim
# symbol class per archive (recorded, not asserted).
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/im-services
cargo build --release -q
./target/release/hematite compose "$FIX" >/dev/null
grep -q "imview driver HOST" "$FIX/components/imview/src/lib.rs" || { echo "FAIL: authored host clobbered"; exit 1; }
if ! _b=$(./target/release/hematite build "$FIX" --app app --out imsvc 2>&1); then echo "FAIL: build imsvc" >&2; echo "$_b" >&2; exit 1; fi

out=$("$FIX/bin/imsvc" 2>/dev/null)
want="[hi | pong:hello | tick:1 | tick:2 | tick:3 | ticks=3]"
[[ "$out" == "$want" ]] || { echo "FAIL: got '$out', want '$want'"; exit 1; }
echo "ok: wrapper unions cross both ways; 3 async wakes completed on the runtime thread; env block read"

set +e
"$FIX/bin/imsvc" echo-gate >/dev/null 2>&1; rc=$?
set -e
[[ $rc == 7 ]] || { echo "FAIL: echo-gate exit $rc, want 7 (component gate hook)"; exit 1; }
set +e
"$FIX/bin/imsvc" driver-gate >/dev/null 2>&1; rc=$?
set -e
[[ $rc == 3 ]] || { echo "FAIL: driver-gate exit $rc, want 3 (driver's own arm after the chain)"; exit 1; }
echo "ok: gate chain — component answered echo-gate (7), driver answered driver-gate (3)"

# (5) allocator shims: which class does each archive define them in? Feeds
# D-H7-11 (per-archive allocators) and the H0c exemption model.
for a in "$FIX"/platform/targets/arm64mac/lib*.a; do
	cls=$( { nm -m "$a" 2>/dev/null || true; } | { grep -E " ___(rust_alloc|rdl_alloc|rg_oom)\b" || true; } | sed -E 's/^[^)]*\) ([a-z ]+external|non-external).*/\1/' | sort -u | tr '\n' ',' )
	echo "   $(basename "$a"): rust_alloc shims = ${cls:-absent}"
done
echo "H7 P0 (service components: unions, wake, env, gates) PASS"

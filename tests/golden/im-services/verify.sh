#!/usr/bin/env bash
# H7 service components (plan 2026-09-09, D-H7-5/6/7/8/13): a reactor driver
# + two service components. The driver ships Cmd/Event/Env with splice markers
# trantor fills; the abi crate's generated services.rs shim carries the
# contract. Asserts: (1) the wrapper unions cross OUT (List(Cmd)) and IN
# (Event) of Roc; (2) HostCtx.wake from a worker thread -> runtime-thread
# complete; (3) the env block; (4) the gate hook chain; (5) records the
# allocator-shim symbol class per archive (one first-wins v0-mangled symbol —
# the reason the driver archive is linked first).
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/im-services
cargo build --release -q
./target/release/trantor compose "$FIX" >/dev/null
grep -q "imview driver HOST" "$FIX/components/imview/src/lib.rs" || { echo "FAIL: authored host clobbered"; exit 1; }
if ! _b=$(./target/release/trantor build "$FIX" --app app --out imsvc 2>&1); then echo "FAIL: build imsvc" >&2; echo "$_b" >&2; exit 1; fi

out=$("$FIX/target/trantor/im-services/bin/imsvc" 2>/dev/null)
want="[hi | pong:hello | rang:ding, in a string long enough to live on the heap:9 | poked from a string long enough to live on the heap | struck:5:struck, in a string long enough to live on the heap | tick:1 | tick:2 | tick:3 | ticks=3]"
[[ "$out" == "$want" ]] || { echo "FAIL: got '$out', want '$want'"; exit 1; }
echo "ok: wrapper unions cross both ways; 3 async wakes completed on the runtime thread; env block read"
echo "ok: one-variant service unions work — a three-field command whose first field is Str, beside a two-field event (Bell); a no-payload command and a one-field event (Nudge) (D-H7-44); a one-record-field command and event glue names itself (Chime) (D-H7-45)"

set +e
"$FIX/target/trantor/im-services/bin/imsvc" echo-gate >/dev/null 2>&1; rc=$?
set -e
[[ $rc == 7 ]] || { echo "FAIL: echo-gate exit $rc, want 7 (component gate hook)"; exit 1; }
set +e
"$FIX/target/trantor/im-services/bin/imsvc" driver-gate >/dev/null 2>&1; rc=$?
set -e
[[ $rc == 3 ]] || { echo "FAIL: driver-gate exit $rc, want 3 (driver's own arm after the chain)"; exit 1; }
echo "ok: gate chain — component answered echo-gate (7), driver answered driver-gate (3)"
dir=$("$FIX/target/trantor/im-services/bin/imsvc" data-dir-gate 2>/dev/null)
[[ "$dir" == "/fixture/data" ]] || { echo "FAIL: data-dir-gate printed '$dir', want '/fixture/data' (HostCtx.data_dir)"; exit 1; }
echo "ok: HostCtx.data_dir — the component read the directory the driver declared"

# (5) allocator shims: which class does each archive define them in? Feeds
# D-H7-11 (per-archive allocators) and the H0c exemption model.
for a in "$FIX"/platform/targets/arm64mac/lib*.a; do
	cls=$( { nm -m "$a" 2>/dev/null || true; } | { grep -E "___rust_alloc$" || true; } | sed -E 's/^[^)]*\) ([a-z ]+external|non-external).*/\1/' | sort -u | tr '\n' ',' )
	echo "   $(basename "$a"): rust_alloc shims = ${cls:-absent}"
done
echo "H7 service components (spliced unions, generated shim, wake, env, gates) PASS"

#!/usr/bin/env bash
# SQ0 acceptance (the roc:sqlite-unsound go/no-go): the two load-bearing
# mechanisms proven on hematite's pinned compiler, before any DB.
#   (A) A host-returned BORROWED slice (rc==0 immortal): used twice, so Roc
#       increfs it once and decrefs it twice against a read-only static backing —
#       an errant write to the refcount word would segfault. Surviving + correct
#       bytes proves the borrow mechanism works here.
#   (B) The host invokes a boxed Roc closure (erased callable) and gets 42.
#   Plus alloc balance: the borrow adds no alloc/free (live=0).
set -euo pipefail
cd "$(dirname "$0")/../../.."
S=tests/golden/sq0-substrate
ROC="${ROC:-$HOME/.bin/roc}"
cargo build --release -q

# the borrow module ships in the composer-emitted abi (hand-written, not glue).
grep -q 'pub mod borrow' src/codegen.rs || { echo "FAIL: composer emits no borrow module"; exit 1; }

if ! _b=$(./target/release/hematite build "$S" --app app --out sq0 2>&1); then echo "FAIL: build sq0" >&2; echo "$_b" >&2; exit 1; fi
# the glue generated the erased-callable ABI (apply_i64! takes a Box(closure)).
grep -q 'RocErasedCallableFn' "$S/target/hematite/sq0-substrate/abi/src/generated.rs" || { echo "FAIL: no erased-callable ABI generated"; exit 1; }

set +e; out=$(cd "$S" && ./target/hematite/sq0-substrate/bin/sq0 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: sq0 exit $code (a borrow free/segfault or a bad erased call)"; echo "$out"; exit 1; }
want=$'borrowed-helloborrowed-hello\n42'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: borrowed slice read twice (incref+decref on read-only rc==0 static, no segfault); erased callable returned 42"

# The borrow must not add an allocation or a free; only the concat result does.
g=$(cd "$S" && HEMATITE_ALLOC_GAUGE=1 ./target/hematite/sq0-substrate/bin/sq0 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$g" || { echo "FAIL: borrow/erased-call leaked or double-freed: $g"; exit 1; }
echo "ok: alloc-gauge balanced — the borrow is immortal, not counted ($g)"
echo "SQ0 PASS"

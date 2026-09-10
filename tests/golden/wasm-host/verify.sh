#!/usr/bin/env bash
# wasm32 (plan 2026-09-09 H7, D-H7-9 revised): `hematite build --target wasm32`
# builds every component for wasm32-unknown-unknown, merges the archives into
# ONE host.wasm (driver whole, each component rooted by its contract members),
# links through roc, and the module runs under node: the app calls component
# b's hosted leaf, b allocates through driver a's roc_alloc.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/wasm-host
cargo build --release -q
if ! _b=$(./target/release/hematite build "$FIX" --target wasm32 --app app --out gz 2>&1); then echo "FAIL: build wasm-host" >&2; echo "$_b" >&2; exit 1; fi
[[ -f "$FIX/target/hematite/wasm-host/bin/gz.wasm" ]] || { echo "FAIL: no $FIX/target/hematite/wasm-host/bin/gz.wasm"; exit 1; }
if ! _r=$(node "$FIX/run.mjs" "$FIX/target/hematite/wasm-host/bin/gz.wasm" 2>&1); then echo "FAIL: run" >&2; echo "$_r" >&2; exit 1; fi
grep -q 'message: "hematite wasm host seed=21"' <<<"$_r" || { echo "FAIL: message"; echo "$_r"; exit 1; }
grep -q '^n: 42' <<<"$_r" || { echo "FAIL: n"; echo "$_r"; exit 1; }
echo "ok: two archives merged into one host.wasm; app -> b -> a resolved; module ran"
echo "H7 wasm32 (merged host, hosted leaf in a second archive) PASS"

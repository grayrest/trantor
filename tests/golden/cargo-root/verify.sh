#!/usr/bin/env bash
# cargo_root (plan 2026-09-09 H7, D-H7-14): a world whose `path` components
# are members of a HOST workspace builds them there — no per-world workspace,
# the world's abi patched in per build — and stages/links as usual.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/cargo-root
cargo build --release -q
./target/release/trantor compose "$FIX/world" >/dev/null
[[ ! -f "$FIX/world/Cargo.toml" ]] || { echo "FAIL: a workspace was generated under cargo_root"; exit 1; }
grep -q "Authored CLI driver" "$FIX/crates/drv/src/lib.rs" || { echo "FAIL: path driver clobbered"; exit 1; }
if ! _b=$(./target/release/trantor build "$FIX/world" --app app --out seed 2>&1); then echo "FAIL: build" >&2; echo "$_b" >&2; exit 1; fi
[[ -f "$FIX/target/release/libcr_svc.a" ]] || { echo "FAIL: archive not built in the host workspace"; exit 1; }
set +e; "$FIX/target/trantor/world/bin/seed"; rc=$?; set -e
[[ $rc == 21 ]] || { echo "FAIL: exit $rc, want 21"; exit 1; }
# The host workspace's own lint resolves the abi through its default patch.
( cd "$FIX" && cargo clippy -q --workspace -- -D warnings ) || { echo "FAIL: host workspace clippy"; exit 1; }
echo "ok: built in the host workspace with the world's abi patched in; exit 21; host clippy clean"
echo "H7 cargo_root PASS"

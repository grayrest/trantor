#!/usr/bin/env bash
# H7 P0 spike (plan 2026-09-09, D-H7-9 / R-H7-2): can roc's wasm32 target take
# MORE THAN ONE host input — one per component — instead of a single pre-merged
# host.wasm? Mirrors roc-solid's wasm/gatezero build with the host split in
# two: `a` (runtime + wasm_main exports) and `b` (the hosted leaf, which
# allocates through a's roc_alloc).
#
# MEASURED (pinned roc-b07d7e-rebased-main):
#   - two `wasm-ld -r` relocatables in `inputs`  -> duplicate symbols
#     (`__negdf2`… every std / compiler-builtins member is strong in both);
#   - two wasm-member ARCHIVES in `inputs`        -> the same: roc links its
#     inputs `--whole-archive`, so archive laziness never applies;
#   - ONE host.wasm merged here (below) with the driver whole and each
#     component rooted by the members defining its contract symbols, the rest
#     lazy                                          -> links and runs.
# So this script's final form is the merge; the two negatives are kept as
# comments because they are the evidence for the D-H7-9 decision.
set -euo pipefail
cd "$(dirname "$0")"
ROC="${ROC:-$HOME/.bin/roc}"
RUST_GLUE="${RUST_GLUE:-$HOME/.bin/RustGlue.roc}"
AR="${AR:-$(command -v llvm-ar || echo /opt/homebrew/opt/llvm/bin/llvm-ar)}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }

echo "== 1. glue =="
rm -rf build glue-out; mkdir -p build glue-out platform/targets/wasm32
cap "$ROC" glue "$RUST_GLUE" glue-out platform/main.roc >/dev/null
cp glue-out/roc_platform_abi.rs a/roc_platform_abi.rs

relocatable() { # <name> <src.rs>: staticlib -> wasm-only members -> lib<name>.wasm
    local name=$1 src=$2
    rustc --edition=2021 --target wasm32-unknown-unknown -C panic=abort --crate-type staticlib -C opt-level=3 "$src" -o "build/$name.a"
    rm -rf "build/$name-members"; mkdir -p "build/$name-members"
    "$AR" x --output="build/$name-members" "build/$name.a"
    local members=()
    for m in "build/$name-members"/*; do
        [ -f "$m" ] || continue
        [ "$(head -c 4 "$m" | xxd -p)" = "0061736d" ] && members+=("$m")
    done
    rm -f "build/lib$name.a"
    "$AR" rcs "build/lib$name.a" "${members[@]}"
    echo "   lib$name.a: $(wc -c < "build/lib$name.a") bytes (${#members[@]} members)"
}
echo "== 2. one wasm-member archive per component =="
relocatable a a/host.rs
relocatable b b/b.rs

echo "== 3. merge: driver whole, component rooted by its contract members =="
# `-r` cannot take `--undefined`, so a component's contract symbols are rooted
# by naming the members that define them (found with llvm-nm); everything else
# in the component archive stays lazy, which is what keeps std and the
# compiler builtins single-copy. trantor knows every contract symbol, so this
# is a deterministic recipe, not a heuristic.
NM="${NM:-$(command -v llvm-nm || echo /opt/homebrew/opt/llvm/bin/llvm-nm)}"
roots=()
for m in build/b-members/*; do
    [ -f "$m" ] || continue
    "$NM" --defined-only "$m" 2>/dev/null | grep -q " T trantor__b__seed$" && roots+=("$m")
done
echo "   b contract members: ${#roots[@]}"
wasm-ld -r --whole-archive build/liba.a --no-whole-archive "${roots[@]}" build/libb.a \
    -o platform/targets/wasm32/host.wasm
echo "   host.wasm: $(wc -c < platform/targets/wasm32/host.wasm) bytes"

echo "== 4. roc build --target=wasm32 =="
cap "$ROC" build --target=wasm32 --no-cache app/main.roc
mv -f main.wasm build/app.wasm 2>/dev/null || mv -f app/main.wasm build/app.wasm
echo "   app.wasm: $(wc -c < build/app.wasm) bytes"

echo "== 5. run =="
node run.mjs

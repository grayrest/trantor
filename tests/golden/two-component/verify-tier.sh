#!/usr/bin/env bash
# H6 acceptance: publish a baseline, classify extensions, and build a Tier-1
# (pure-Roc) extension from the published baseline with NO Rust toolchain.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/two-component
cargo build --release -q

if ! _b=$(./target/release/trantor build "$FIX" --app app --out reader 2>&1); then echo "FAIL: build reader (baseline archives)" >&2; echo "$_b" >&2; exit 1; fi      # baseline archives
./target/release/trantor publish "$FIX" >/dev/null 2>&1
grep -q abi_fingerprint "$FIX/target/trantor/two-component/dist/baseline.lock" || { echo "FAIL: no fingerprint"; exit 1; }
echo "ok: baseline published with ABI fingerprint"

t1=$(./target/release/trantor tier "$FIX" --world extensions/tier1.toml)
t2=$(./target/release/trantor tier "$FIX" --world extensions/tier2.toml)
[[ "$t1" == Tier\ 1:* ]] || { echo "FAIL: pure-Roc ext not Tier 1: $t1"; exit 1; }
[[ "$t2" == Tier\ 2:* ]] || { echo "FAIL: host ext not Tier 2: $t2"; exit 1; }
echo "ok: pure-Roc extension -> Tier 1; host extension -> Tier 2"

# A reader that goes away closes the pipe under trantor. Rust ignores SIGPIPE
# by default, so the next `println!` used to panic and print a backtrace note
# where `cat` prints nothing; main.rs restores the default disposition.
#
# The reader is `(exit 0)`, not `head -1`: a pipe buffers 64KB, so `head`
# takes both of tier's lines and closes only after trantor is done writing —
# it loses the race almost every time and the check passes either way. A
# reader that exits before trantor has parsed a manifest never does.
# `|| true` because the fix's own success is a nonzero status: trantor is now
# killed by SIGPIPE, the pipeline reports 141 under `pipefail`, and `set -e`
# would take the script down at this assignment.
piped_err=$( { ./target/release/trantor tier "$FIX" --world extensions/tier1.toml | (exit 0); } 2>&1 >/dev/null ) || true
[[ -z "$piped_err" ]] || { echo "FAIL: trantor wrote to stderr when its reader went away: ${piped_err:0:120}"; exit 1; }
# ...and the verdict still reaches a reader that stays. Without this, a trantor
# that printed nothing at all would satisfy the assertion above.
first=$(./target/release/trantor tier "$FIX" --world extensions/tier1.toml | head -1)
[[ "$first" == Tier\ 1:* ]] || { echo "FAIL: nothing reached a reader through the pipe — got: $first"; exit 1; }
echo "ok: a reader that goes away gets no panic; one that stays gets the verdict"

# Tier-1 build from the PUBLISHED dist, no cargo/glue.
T=$(mktemp -d)
cp -r "$FIX/target/trantor/two-component/dist" "$T/baseline"   # D-H7-38 moved this; line 11 was updated and this one was not
cat > "$T/baseline/platform/Greet.roc" <<'ROC'
Greet :: [].{
	banner : Str -> Str
	banner = |n| Str.concat("~ ", Str.concat(n, " ~"))
}
ROC
perl -pi -e 's/exposes \[Path, Stdio, Env\]/exposes [Path, Stdio, Env, Greet]/; s/^import Env$/import Env\nimport Greet/' "$T/baseline/platform/main.roc"
mkdir -p "$T/app" "$T/target/trantor/two-component/bin"   # D-H7-38 moved bin/ too
cat > "$T/app/main.roc" <<'ROC'
app [main!] { pf: platform "../baseline/platform/main.roc" }
import pf.Stdio
import pf.Greet
main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| { Stdio.line!(Greet.banner("per-app world")) ?? {} 
	Ok({}) }
ROC
perl -e 'alarm shift; exec @ARGV' 120 "$HOME/.bin/roc" build --output="$T/target/trantor/two-component/bin/app" "$T/app/main.roc" >/dev/null 2>&1
out=$("$T/target/trantor/two-component/bin/app")
[[ "$out" == "~ per-app world ~" ]] || { echo "FAIL: tier-1 build output '$out'"; exit 1; }
echo "ok: Tier-1 extension built from published baseline with only roc build — '$out'"
rm -rf "$T"
echo "H6 PASS"

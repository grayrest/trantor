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

# Tier-1 build from the PUBLISHED dist, no cargo/glue.
T=$(mktemp -d)
cp -r "$FIX/target/trantor/two-component/dist" "$T/baseline"   # D-H7-38 moved this; line 11 was updated and this one was not
cat > "$T/baseline/platform/Greet.roc" <<'ROC'
Greet :: [].{
	banner : Str -> Str
	banner = |n| Str.concat("~ ", Str.concat(n, " ~"))
}
ROC
sed -i '' 's/exposes \[Path, Stdio, Env\]/exposes [Path, Stdio, Env, Greet]/; s/^import Env$/import Env\nimport Greet/' "$T/baseline/platform/main.roc"
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

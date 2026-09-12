#!/usr/bin/env bash
# U1 P3/P4 acceptance: a dependency is one line (D-U1-1), it declares itself
# (D-U1-2), and the consumer authors no driver (D-U1-6).
#
# The consumer here is the whole claim: world.toml and app/main.roc, nothing
# else. No [components], no [wiring], no driver, no interfaces directory, no
# Cargo workspace. All of it comes from `base = { path = "pkg" }`.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/u1-deps
cargo build --release -q

# (1) The consumer really is minimal — assert the absence, or the rest of this
# script would pass just as well on a world that declared everything.
# Comments stripped: the manifest EXPLAINS what it does not declare, and the
# first version of this check was reading its own prose.
W=$(grep -v '^[[:space:]]*#' "$FIX/world.toml")
for section in '^\[components' '^\[wiring' '^\[interfaces' '^driver'; do
	if grep -q "$section" <<<"$W"; then
		echo "FAIL: the consumer declares $section — it is supposed to inherit it"; exit 1
	fi
done
files=$(cd "$FIX" && ls -1 world.toml app/main.roc | wc -l | tr -d ' ')
[[ "$files" == 2 ]] || { echo "FAIL: expected 2 authored consumer files"; exit 1; }
echo "ok: the consumer is world.toml + app/main.roc, with one [deps] line"

# (2) Expansion actually contributed something. Without this the checks above
# pass on a world that inherits nothing because the dep was never read.
out=$(./target/release/trantor compose "$FIX" 2>&1)
grep -q "hosted symbols" <<<"$out" || { echo "FAIL: compose said nothing useful: $out"; exit 1; }
n=$(sed -E 's/.*\(([0-9]+) hosted symbols.*/\1/' <<<"$out")
[[ "${n:-0}" -gt 0 ]] || { echo "FAIL: 0 hosted symbols — the dependency contributed nothing: $out"; exit 1; }
echo "ok: the dependency contributed $n hosted symbol(s)"

# (3) The driver came from the package (D-U1-6) and its crate was generated.
G="$FIX/target/trantor/u1-deps"
[[ -f "$G/components/drv/src/lib.rs" ]] || { echo "FAIL: no driver crate generated from the dep's driver.toml"; exit 1; }
[[ -f "$G/components/text-host/src/lib.rs" ]] || { echo "FAIL: the dep's host crate was not brought into the workspace"; exit 1; }
grep -q 'patch.crates-io' "$G/Cargo.toml" || { echo "FAIL: no abi patch for the package's crate"; exit 1; }
echo "ok: driver generated from the dep's driver.toml; its host crate is a workspace member"

# (4) It builds and runs.
./target/release/trantor build "$FIX" --app app --out shout >/dev/null 2>&1 || { echo "FAIL: build"; exit 1; }
got=$("$G/bin/shout")
[[ "$got" == "58" ]] || { echo "FAIL: got '$got', want '58'"; exit 1; }
echo "ok: builds and runs — the app reached the dependency's host code"

# (5) A path dep touches no network. Point the cache at an empty directory and
# require that nothing appears in it.
T=$(mktemp -d)
rm -rf "$G"
TRANTOR_HOME="$T" ./target/release/trantor build "$FIX" --app app --out shout >/dev/null 2>&1 \
	|| { rm -rf "$T"; echo "FAIL: offline build"; exit 1; }
if [[ -d "$T/cache" ]]; then rm -rf "$T"; echo "FAIL: a path dep populated the fetch cache"; exit 1; fi
rm -rf "$T"
echo "ok: a path dep resolves with an empty cache and never reaches out"

echo "U1 P3/P4 deps PASS"

#!/usr/bin/env bash
# T3: `trantor test <package>`. A pass proves little on its own — a runner that
# checks nothing passes too — so each check below is also broken on purpose in
# a scratch copy, and the run must fail naming what broke.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=$PWD/tests/golden/t3-package-test
TR=$PWD/target/release/trantor
CLI=$PWD/../trantor-cli
[[ -f "$CLI/package.toml" ]] || { echo "SKIP: needs trantor-cli checked out beside trantor"; exit 0; }
cargo build --release -q
T=$(cd "$(mktemp -d)" && pwd -P); trap 'rm -rf "$T"' EXIT

# A scratch copy of the package with its dev-dep made absolute, then `edit`.
variant() {
	local name=$1 edit=$2
	rm -rf "$T/$name"; cp -R "$FIX/pkg" "$T/$name"
	sed -i '' "s|path = \"../../../../../trantor-cli\"|path = \"$CLI\"|" "$T/$name/package.toml"
	(cd "$T/$name" && eval "$edit")
}

variant pass ':'
if ! "$TR" test "$T/pass" > "$T/pass.out" 2>&1; then echo "FAIL: the clean package does not pass"; tail -30 "$T/pass.out"; exit 1; fi
for claim in "alone it says it has no driver" "composes with its dev-deps" "none of its 1 exported modules" \
	"1 of them this package's own" "README.md — 1 blocks compile and run, 1 stated values match" \
	"tests/hello — 1 lines exact" "the script got TRANTOR, ROC, PKG, DEPS, DEV_DEPS and TMP" "trantor test: greet PASS"; do
	grep -qF "$claim" "$T/pass.out" || { echo "FAIL: a passing run did not report: $claim"; cat "$T/pass.out"; exit 1; }
done
echo "ok: the clean package passes, reporting every step"

breaks() {  # name, edit, the message the failure must contain
	variant "$1" "$2"
	if "$TR" test "$T/$1" > "$T/$1.out" 2>&1; then echo "FAIL: $1 — the broken package passed"; exit 1; fi
	grep -qF "$3" "$T/$1.out" || { echo "FAIL: $1 failed without saying \"$3\""; tail -20 "$T/$1.out"; exit 1; }
}
breaks exports-owned 'sed -i "" "s/^exports = \[\"Greet\"\]/exports = [\"Greet\", \"Stdout\"]/" package.toml' \
	"the dev-deps alone already provide Stdout"
breaks expects-unreached 'sed -i "" "s/^exports = \[\"Greet\"\]/exports = []/" package.toml' \
	"contributed none of the"
breaks readme-value 'sed -i "" "s/# \"hello world\"/# \"hello there\"/" README.md' \
	"README.md says one thing and the package does another"
breaks app-expected 'printf "hello nobody\n" > tests/hello/expected' \
	"tests/hello: output differs"
breaks script-fails 'printf "exit 3\n" >> tests/script/test.sh' \
	"tests/script/test.sh failed"
breaks empty-suite 'mkdir tests/empty' \
	"tests/empty/ is none of"
breaks two-kinds 'cp tests/script/test.sh tests/hello/' \
	"tests/hello/ is more than one kind of suite"
echo "ok: a package owning a baseline module, unreached expects, a wrong README value, a wrong expected line, a failing script, an empty suite and a two-kind suite each fail, and say so"
echo "T3 PASS"

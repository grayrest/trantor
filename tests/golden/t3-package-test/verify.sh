#!/usr/bin/env bash
# T3: `trantor test <package>`. A pass proves little on its own — a runner that
# checks nothing passes too — so each check below is also broken on purpose in
# a scratch copy, and the run must fail naming what broke. Cases that used to
# HANG are run under a deadline and must pass inside it.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=$PWD/tests/golden/t3-package-test
TR=$PWD/target/release/trantor
CLI=$PWD/../trantor-cli
[[ -f "$CLI/package.toml" ]] || { echo "SKIP: needs trantor-cli checked out beside trantor"; exit 0; }
cargo build --release -q
T=$(cd "$(mktemp -d)" && pwd -P); trap 'rm -rf "$T"' EXIT
# trantor test keeps a failed run's scratch worlds; keep them inside $T.
export TMPDIR="$T/tmp"; mkdir -p "$TMPDIR"

# A scratch copy of the package with its dev-dep made absolute, then `edit`.
variant() {
	local name=$1 edit=$2
	rm -rf "$T/$name"; cp -R "$FIX/pkg" "$T/$name"
	perl -pi -e "s|path = \"../../../../../trantor-cli\"|path = \"$CLI\"|" "$T/$name/package.toml"
	(cd "$T/$name" && eval "$edit")
}
# Edit the [package] table only, not a component's `exports`.
pkg_exports() { perl -0pi -e "s/(\[package\][^\[]*?)exports = \[\"Greet\"\]/\${1}exports = [$1]/" package.toml; }

variant pass ':'
if ! "$TR" test "$T/pass" > "$T/pass.out" 2>&1; then echo "FAIL: the clean package does not pass"; tail -30 "$T/pass.out"; exit 1; fi
for claim in "alone it says it has no driver" "composes with its dev-deps" "none of its 1 exported modules" \
	"1 of them this package's own" "README.md — 1 whole apps run (0 with stated output), 0 module-level blocks compile (not run), 1 fragment blocks run; 1 stated values compared" \
	"tests/hello — 1 lines exact" "the script got TRANTOR, ROC, PKG, DEPS, DEV_DEPS and TMP" "trantor test: greet PASS"; do
	grep -qF "$claim" "$T/pass.out" || { echo "FAIL: a passing run did not report: $claim"; cat "$T/pass.out"; exit 1; }
done
echo "ok: the clean package passes, reporting every step"

breaks() {  # name, edit, the message the failure must contain, [VAR=value for the run]
	variant "$1" "$2"
	if env ${4:-} "$TR" test "$T/$1" > "$T/$1.out" 2>&1; then echo "FAIL: $1 — the broken package passed"; exit 1; fi
	grep -qF "$3" "$T/$1.out" || { echo "FAIL: $1 failed without saying \"$3\""; tail -20 "$T/$1.out"; exit 1; }
}
passes_within() {  # name, seconds, edit — must PASS, and before the deadline
	variant "$1" "$3"
	perl -e 'alarm shift; exec @ARGV' "$2" "$TR" test "$T/$1" > "$T/$1.out" 2>&1 \
		|| { echo "FAIL: $1 did not pass within $2 s"; tail -20 "$T/$1.out"; exit 1; }
}

breaks exports-owned 'pkg_exports "\"Greet\", \"Stdout\""' \
	"already provides Stdout"
breaks baseline-in-deps 'pkg_exports "\"Greet\", \"Stdout\""; perl -pi -e "s/^\[dev-deps\]/[deps]/" package.toml' \
	"already provides Stdout"
breaks unexported-expect 'mkdir -p components/hidden-lib
	printf "Hidden :: [].{\n\tanswer : I64\n\tanswer = 41\n}\n\nexpect Hidden.answer == 42\n" > components/hidden-lib/Hidden.roc
	printf "\n[components.hidden-lib]\nkind = \"roc\"\nexports = [\"Hidden\"]\n" >> package.toml' \
	"roc test"
breaks orphan-expect 'printf "Helper :: [].{\n\tx : I64\n\tx = 1\n}\n\nexpect\n\tHelper.x == 2\n" > components/greet-lib/Helper.roc' \
	"Helper is not a module any component exports"
breaks expects-unreached 'pkg_exports ""; perl -pi -e "s/^exports = \[\"Greet\"\]/exports = []/" package.toml' \
	"Greet is not a module any component exports"
breaks stray-world "printf '[world]\nname = \"stray\"\n\n[deps]\ntrantor-cli = { path = \"$CLI\" }\n' > world.toml; printf 'exit 3\n' >> tests/script/test.sh" \
	"tests/script/test.sh failed"
breaks readme-value 'perl -pi -e "s/# \"hello world\"/# \"hello there\"/" README.md' \
	"README.md says one thing and the package does another"
breaks readme-app 'perl -pi -e "s/Greet.hello\(\"reader\"\)/Greet.nope(\"reader\")/" README.md' \
	"the README.md app at line"
breaks readme-near-miss 'perl -pi -e "s/# \"hello world\"/# 42, give or take/" README.md' \
	"is not one trantor can check"
breaks readme-app-output 'printf "\n\`\`\`text\nhello nobody\n\`\`\`\n" >> README.md' \
	"printed something other than the output stated at line"
breaks readme-output-after-prose 'printf "\nIt prints:\n\n\`\`\`text\nhello nobody\n\`\`\`\n" >> README.md' \
	"printed something other than the output stated at line"
breaks readme-bool 'perl -0pi -e "s/(Greet.hello\(who\)   # \"hello world\"\n)/\$1Greet.hello(who) == \"hello nobody\"   # True\n/" README.md' \
	"stated True, actual False"
breaks readme-app-exit 'perl -0pi -e "s/\`\`\`roc\napp/\`\`\`roc exit=3\napp/" README.md' \
	"expected 3"
breaks app-expected 'printf "hello nobody\n" > tests/hello/expected' \
	"tests/hello: output differs"
breaks script-fails 'printf "exit 3\n" >> tests/script/test.sh' \
	"tests/script/test.sh failed"
breaks cargo-empty 'mkdir -p tests/rust/src && printf "[package]\nname = \"empty\"\nversion = \"0.0.0\"\nedition = \"2021\"\n" > tests/rust/Cargo.toml && : > tests/rust/src/lib.rs' \
	"tests/rust: cargo test ran no tests"
# Nothing to build, so only the script can reach the deadline.
breaks script-hangs 'rm -rf README.md tests/hello; printf "sleep 600\n" >> tests/script/test.sh' \
	"tests/script/test.sh failed: killed after 30 s" TRANTOR_TEST_TIMEOUT=30
breaks bad-timeout ':' \
	"expected whole seconds" TRANTOR_TEST_TIMEOUT=15m
breaks symlinked-orphan 'mv components/greet-lib "$T/greet-lib-$RANDOM" && ln -s "$(ls -d "$T"/greet-lib-* | tail -1)" components/greet-lib
	printf "Helper :: [].{\n\tx : I64\n\tx = 1\n}\n\nexpect Helper.x == 2\n" > components/greet-lib/Helper.roc' \
	"Helper is not a module any component exports"
breaks module-clash 'mkdir -p components/mine-lib && printf "Stdout :: [].{\n\tx : I64\n\tx = 1\n}\n" > components/mine-lib/Stdout.roc
	printf "\n[components.mine-lib]\nkind = \"roc\"\nexports = [\"Stdout\"]\n" >> package.toml' \
	"two sources for the platform module \`Stdout\`"
breaks empty-suite 'mkdir tests/empty' \
	"tests/empty/ is none of"
breaks two-kinds 'cp tests/script/test.sh tests/hello/' \
	"tests/hello/ is more than one kind of suite"
echo "ok: 22 broken packages each fail naming the break — a baseline module claimed (via dev-deps and via deps), an expect in an unexported module, an expect in no module, unreached expects, a stray world.toml, a README value, a README app, a README near-miss value, a README app's stated output (directly after it and after prose), a stated Bool, a README app's exit status, an expected line, a failing script, an empty cargo suite, a hung script, a malformed timeout, an orphan expect behind a symlink, a module from two sources, an empty suite, a two-kind suite"

passes_within symlink-loop 240 'ln -s . loop1; ln -s . loop2'
passes_within background-child 240 'printf "(sleep 600) &\n" > tests/script/prelude.sh && perl -0pi -e "s/set -euo pipefail\n/set -euo pipefail\nsource prelude.sh\n/" tests/script/test.sh'
passes_within hidden-dir 240 'mkdir -p tests/.cache'
echo "ok: a symlink loop, a script leaving a child running, and a hidden tests/ directory each pass without hanging"

# An interrupted run takes its subprocesses with it: they are in their own
# process groups, where a terminal's Ctrl-C does not reach them.
variant interrupted 'rm -rf README.md tests/hello; printf "echo \$\$ > \"%s/sleeper.pid\"\nexec sleep 600\n" "$T" >> tests/script/test.sh'
"$TR" test "$T/interrupted" > "$T/interrupted.out" 2>&1 & runner=$!
for _ in $(seq 1200); do [[ -s "$T/sleeper.pid" ]] && break; sleep 0.1; done
sleeper=$(cat "$T/sleeper.pid" 2>/dev/null) || { echo "FAIL: the interrupted run never reached its script"; tail -20 "$T/interrupted.out"; exit 1; }
kill -INT "$runner"; wait "$runner" 2>/dev/null || true
sleep 1
if kill -0 "$sleeper" 2>/dev/null; then kill "$sleeper"; echo "FAIL: an interrupted trantor test left its script running"; exit 1; fi
echo "ok: an interrupted run ends the script it was running"
echo "T3 PASS"

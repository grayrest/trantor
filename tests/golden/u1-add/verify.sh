#!/usr/bin/env bash
# `trantor add <org>/<repo> [<dir>]`: adds to the project here — a world or a
# package — pins it, composes, and changes NOTHING when that composition fails.
#
# No network: git's `url.<base>.insteadOf`, set through GIT_CONFIG_* for this
# process tree only, points https://github.com/org/ at local repositories.
set -euo pipefail
cd "$(dirname "$0")/../../.."
TR=$PWD/target/release/trantor
CLI=$PWD/../trantor-cli
[[ -f "$CLI/package.toml" ]] || { echo "SKIP: needs trantor-cli checked out beside trantor"; exit 0; }
cargo build --release -q
T=$(cd "$(mktemp -d)" && pwd -P); trap 'rm -rf "$T"' EXIT
export TRANTOR_HOME="$T/home" TMPDIR="$T/tmp"; mkdir -p "$TMPDIR"
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.file://$T/remotes/.insteadOf" GIT_CONFIG_VALUE_0="https://github.com/org/"

# A tagged local "github" repository holding a package.
remote() {  # name, package.toml body
	local d="$T/src/$1"; mkdir -p "$d"
	printf '%s\n' "$2" > "$d/package.toml"
	git -C "$d" init -q && git -C "$d" add -A && git -C "$d" -c user.email=t@t -c user.name=t commit -qm init && git -C "$d" tag v0.1.0
	git clone -q --bare "$d" "$T/remotes/$1.git"
}
# A pure-Roc add-on: nothing to build, so composing it is quick.
remote greet '[package]
name = "greet"
exports = ["Greet"]

[components.greet-lib]
kind = "roc"
exports = ["Greet"]'
mkdir -p "$T/src/greet/components/greet-lib"
printf 'Greet :: [].{\n\thello : Str -> Str\n\thello = |n| "hello ${n}"\n}\n' > "$T/src/greet/components/greet-lib/Greet.roc"
git -C "$T/src/greet" add -A && git -C "$T/src/greet" -c user.email=t@t -c user.name=t commit -qm module && git -C "$T/src/greet" tag -f v0.1.1 >/dev/null
rm -rf "$T/remotes/greet.git" && git clone -q --bare "$T/src/greet" "$T/remotes/greet.git"
# Parses and fetches, but cannot compose: it wires an interface to a component
# that does not exist.
remote broken '[package]
name = "broken"

[provides]
nothing = "no-such-component"

[interfaces.nothing]'

sum() { shasum "$@" 2>/dev/null | cut -d' ' -f1 | tr '\n' ' '; }

# (1) A world, from here, with no <dir>.
"$TR" new "$T/app" --cli --from "$CLI" >/dev/null 2>&1
(cd "$T/app" && "$TR" add org/greet) > "$T/add1.out" 2>&1 || { echo "FAIL: add to a world"; cat "$T/add1.out"; exit 1; }
grep -q '^greet = { github = "org/greet" }' "$T/app/world.toml" || { echo "FAIL: world.toml has no greet line"; cat "$T/app/world.toml"; exit 1; }
grep -q 'tag = "v0.1.1"' "$T/app/trantor.lock" || { echo "FAIL: not pinned to the newest tag"; cat "$T/app/trantor.lock"; exit 1; }
grep -q 'Greet' "$T/app/target/trantor/app/platform/main.roc" || { echo "FAIL: add did not compose — Greet is not exposed"; exit 1; }
echo "ok: add in a world edits world.toml, pins the newest tag, and composes"

# (2) Again: a no-op.
before=$(sum "$T/app/world.toml" "$T/app/trantor.lock")
"$TR" add org/greet "$T/app" > "$T/add2.out" 2>&1 || { echo "FAIL: second add"; cat "$T/add2.out"; exit 1; }
[[ "$(sum "$T/app/world.toml" "$T/app/trantor.lock")" == "$before" ]] || { echo "FAIL: a second add changed a byte"; exit 1; }
grep -q "lock unchanged" "$T/add2.out" || { echo "FAIL: a second add did not say the lock was unchanged"; cat "$T/add2.out"; exit 1; }
echo "ok: a second add, with <dir> given, changes no byte"

# (3) A dependency that cannot compose leaves both files exactly as they were.
if (cd "$T/app" && "$TR" add org/broken) > "$T/add3.out" 2>&1; then echo "FAIL: add of an uncomposable package succeeded"; exit 1; fi
[[ "$(sum "$T/app/world.toml" "$T/app/trantor.lock")" == "$before" ]] || { echo "FAIL: a failed add left world.toml or trantor.lock edited"; exit 1; }
grep -q "world.toml and trantor.lock are unchanged" "$T/add3.out" || { echo "FAIL: a failed add did not say it changed nothing"; cat "$T/add3.out"; exit 1; }
echo "ok: an add whose composition fails changes nothing, and says so"

# (4) A package, from here: its own package.toml and its own lock.
mkdir -p "$T/mine/components/mine-lib"
printf '[package]\nname = "mine"\nexports = ["Mine"]\n\n[components.mine-lib]\nkind = "roc"\nexports = ["Mine"]\n\n[dev-deps]\ntrantor-cli = { path = "%s" }\n' "$CLI" > "$T/mine/package.toml"
printf 'Mine :: [].{\n\tx : Str\n\tx = "x"\n}\n' > "$T/mine/components/mine-lib/Mine.roc"
(cd "$T/mine" && "$TR" add org/greet) > "$T/add4.out" 2>&1 || { echo "FAIL: add to a package"; cat "$T/add4.out"; exit 1; }
grep -q '^greet = { github = "org/greet" }' "$T/mine/package.toml" || { echo "FAIL: package.toml has no greet line"; cat "$T/mine/package.toml"; exit 1; }
[[ -f "$T/mine/trantor.lock" && ! -f "$T/mine/world.toml" ]] || { echo "FAIL: the package got no lock, or got a world"; exit 1; }
pbefore=$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")
if (cd "$T/mine" && "$TR" add org/broken) > "$T/add5.out" 2>&1; then echo "FAIL: add of an uncomposable package to a package succeeded"; exit 1; fi
[[ "$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")" == "$pbefore" ]] || { echo "FAIL: a failed add left package.toml or its lock edited"; exit 1; }
echo "ok: add in a package edits package.toml and its own lock, and a failing one changes nothing"

# (5) The old argument order says what the new one is.
if "$TR" add "$T/app" org/greet > "$T/add6.out" 2>&1; then echo "FAIL: add <dir> <org/repo> was accepted"; exit 1; fi
grep -q "trantor add <org>/<repo> \[<dir>\]" "$T/add6.out" || { echo "FAIL: the old order did not get the usage"; cat "$T/add6.out"; exit 1; }
echo "ok: the old <dir> <org/repo> order is refused with the usage"

# (6) new without a baseline writes no app and says how to get one.
"$TR" new "$T/bare" > "$T/new.out" 2>&1
[[ ! -e "$T/bare/app/main.roc" ]] || { echo "FAIL: new with no baseline wrote an app"; exit 1; }
grep -q -- "--from" "$T/new.out" || { echo "FAIL: new with no baseline did not point at --from"; cat "$T/new.out"; exit 1; }
echo "ok: new with no baseline writes no app and points at --from"
echo "U1-ADD PASS"

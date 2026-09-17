#!/usr/bin/env bash
# `trantor new --from` writes an app that typechecks, and
# `trantor add <org>/<repo> [<dir>]` adds to the project here — a world or a
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
# mktemp alone first: inside `cd "$(mktemp -d)"` a failed mktemp leaves `cd ""`,
# which macOS /bin/bash and Ubuntu's bash accept as staying put, so T
# became the repo root, which the EXIT trap then deleted.
T=$(mktemp -d); T=$(cd "$T" && pwd -P); trap 'rm -rf "$T"' EXIT
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
# Export a module they do not ship, and one another package already ships.
remote ghost '[package]
name = "ghost"
exports = ["Ghost"]

[components.ghost-lib]
kind = "roc"
exports = ["Ghost"]'
remote clash '[package]
name = "clash"
exports = ["Mine"]

[components.clash-lib]
kind = "roc"
exports = ["Mine"]'
mkdir -p "$T/src/clash/components/clash-lib" && printf 'Mine :: [].{\n\ty : Str\n\ty = "y"\n}\n' > "$T/src/clash/components/clash-lib/Mine.roc"
git -C "$T/src/clash" add -A && git -C "$T/src/clash" -c user.email=t@t -c user.name=t commit -qm module && git -C "$T/src/clash" tag -f v0.1.1 >/dev/null
rm -rf "$T/remotes/clash.git" && git clone -q --bare "$T/src/clash" "$T/remotes/clash.git"
# Parses and fetches, but cannot compose: it declares an interface it does not
# ship (no interfaces/nothing/interface.toml).
remote broken '[package]
name = "broken"

[provides]
nothing = "no-such-component"

[interfaces.nothing]'

sum() { shasum "$@" 2>/dev/null | cut -d' ' -f1 | tr '\n' ' '; }

# (1) A world, from here, with no <dir>.
"$TR" new "$T/app" --cli --from "$CLI" >/dev/null 2>&1
# The scaffold is typechecked untouched against trantor-cli, whose driver does
# not take the signature the scaffold used to hardcode. u1-front-door's own
# baseline does, so only a real baseline can catch that regression.
"$TR" check "$T/app" > "$T/check.out" 2>&1 || { echo "FAIL: the app trantor new wrote does not typecheck against trantor-cli"; tail -15 "$T/check.out"; exit 1; }
echo "ok: the app trantor new --from trantor-cli writes typechecks untouched"
(cd "$T/app" && "$TR" add org/greet) > "$T/add1.out" 2>&1 || { echo "FAIL: add to a world"; cat "$T/add1.out"; exit 1; }
grep -q '^greet = { github = "org/greet" }' "$T/app/world.toml" || { echo "FAIL: world.toml has no greet line"; cat "$T/app/world.toml"; exit 1; }
grep -q 'tag = "v0.1.1"' "$T/app/trantor.lock" || { echo "FAIL: not pinned to the newest tag"; cat "$T/app/trantor.lock"; exit 1; }
grep -q 'Greet' "$T/app/target/trantor/app/platform/main.roc" || { echo "FAIL: add did not compose — Greet is not exposed"; exit 1; }
echo "ok: add in a world edits world.toml, pins the newest tag, and composes"

# (2) Again: a no-op.
before=$(sum "$T/app/world.toml" "$T/app/trantor.lock")
"$TR" add org/greet "$T/app" > "$T/add2.out" 2>&1 || { echo "FAIL: second add"; cat "$T/add2.out"; exit 1; }
[[ "$(sum "$T/app/world.toml" "$T/app/trantor.lock")" == "$before" ]] || { echo "FAIL: a second add changed a byte"; exit 1; }
grep -q "nothing changed" "$T/add2.out" || { echo "FAIL: a second add did not say nothing changed"; cat "$T/add2.out"; exit 1; }
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

# (6) One name is one package, and one package one name.
if (cd "$T/app" && "$TR" add org/broken --as greet) > "$T/add7.out" 2>&1; then echo "FAIL: add replaced the dep named greet"; exit 1; fi
grep -q "already a dependency" "$T/add7.out" || { echo "FAIL: a name clash was not named"; cat "$T/add7.out"; exit 1; }
if (cd "$T/app" && "$TR" add org/greet --as greet2) > "$T/add8.out" 2>&1; then echo "FAIL: add took a package already present under another name"; exit 1; fi
grep -q "already a dependency in world.toml, as \`greet\`" "$T/add8.out" || { echo "FAIL: the duplicate was not named"; cat "$T/add8.out"; exit 1; }
[[ "$(sum "$T/app/world.toml" "$T/app/trantor.lock")" == "$before" ]] || { echo "FAIL: a refused add changed a byte"; exit 1; }
echo "ok: add refuses a taken name and a package already present under another name"

# (7) update and remove work on a package, in add's argument order.
"$TR" update greet "$T/mine" > "$T/up1.out" 2>&1 || { echo "FAIL: update in a package"; cat "$T/up1.out"; exit 1; }
grep -q "already at" "$T/up1.out" || { echo "FAIL: update did not report the pin"; cat "$T/up1.out"; exit 1; }
if "$TR" update "$T/mine" greet > "$T/up2.out" 2>&1; then echo "FAIL: update <dir> <name> was accepted"; exit 1; fi
grep -q "the name comes first" "$T/up2.out" || { echo "FAIL: the old update order did not say which comes first"; cat "$T/up2.out"; exit 1; }
(cd "$T/mine" && "$TR" remove greet) > "$T/rm1.out" 2>&1 || { echo "FAIL: remove in a package"; cat "$T/rm1.out"; exit 1; }
! grep -q greet "$T/mine/package.toml" "$T/mine/trantor.lock" || { echo "FAIL: remove left greet in package.toml or its lock"; exit 1; }
echo "ok: update and remove work on a package, and the old <dir> <name> order is refused"

# (8) An add killed while it composes is undone by the next command there.
cp "$T/mine/components/mine-lib/Mine.roc" "$T/Mine.roc.keep"
rm "$T/mine/components/mine-lib/Mine.roc" && mkfifo "$T/mine/components/mine-lib/Mine.roc"
pbefore=$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")
(cd "$T/mine" && exec "$TR" add org/greet) > "$T/add9.out" 2>&1 & adder=$!
for _ in $(seq 100); do [[ -d "$T/mine/.trantor-edit" ]] && grep -q greet "$T/mine/package.toml" && break; sleep 0.1; done
grep -q greet "$T/mine/package.toml" || { kill -KILL "$adder" 2>/dev/null; echo "FAIL: the add never reached its compose"; cat "$T/add9.out"; exit 1; }
sleep 0.5; kill -KILL "$adder"; wait "$adder" 2>/dev/null || true
rm "$T/mine/components/mine-lib/Mine.roc" && cp "$T/Mine.roc.keep" "$T/mine/components/mine-lib/Mine.roc"
"$TR" remove nothing-here "$T/mine" > "$T/rec.out" 2>&1 || true
grep -q "was undone" "$T/rec.out" || { echo "FAIL: the next command did not undo the killed add"; cat "$T/rec.out"; exit 1; }
[[ "$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")" == "$pbefore" && ! -e "$T/mine/.trantor-edit" ]] || { echo "FAIL: the killed add's edit survived"; exit 1; }
# Killed again, then edited by hand before the next command: the edit is kept.
rm "$T/mine/components/mine-lib/Mine.roc" && mkfifo "$T/mine/components/mine-lib/Mine.roc"
(cd "$T/mine" && exec "$TR" add org/greet) > "$T/add9b.out" 2>&1 & adder=$!
for _ in $(seq 100); do [[ -d "$T/mine/.trantor-edit" ]] && grep -q greet "$T/mine/package.toml" && break; sleep 0.1; done
sleep 0.5; kill -KILL "$adder"; wait "$adder" 2>/dev/null || true
rm "$T/mine/components/mine-lib/Mine.roc" && cp "$T/Mine.roc.keep" "$T/mine/components/mine-lib/Mine.roc"
printf '# my hand edit\n' >> "$T/mine/package.toml"
"$TR" remove nothing-here "$T/mine" > "$T/rec2.out" 2>&1 || true
grep -q "changed since" "$T/rec2.out" || { echo "FAIL: recovery did not say the manifest changed since"; cat "$T/rec2.out"; exit 1; }
grep -q "my hand edit" "$T/mine/package.toml" || { echo "FAIL: recovery reverted a hand edit made after the crash"; exit 1; }
rm -rf "$T/mine/.trantor-edit"
echo "ok: an add killed mid-compose is undone by the next trantor command, unless the files changed since"

# (9) A package that cannot be composed cannot have a change checked: refused.
mkdir -p "$T/addon/components/addon-lib"
printf '[package]\nname = "addon"\nexports = ["Addon"]\n\n[components.addon-lib]\nkind = "roc"\nexports = ["Addon"]\n' > "$T/addon/package.toml"
printf 'Addon :: [].{\n\tx : Str\n\tx = "x"\n}\n' > "$T/addon/components/addon-lib/Addon.roc"
abefore=$(sum "$T/addon/package.toml")
if (cd "$T/addon" && "$TR" add org/broken) > "$T/add10.out" 2>&1; then echo "FAIL: add to a package with no baseline succeeded"; exit 1; fi
grep -q "\[dev-deps\]" "$T/add10.out" || { echo "FAIL: the refusal did not say to name a baseline"; cat "$T/add10.out"; exit 1; }
[[ "$(sum "$T/addon/package.toml")" == "$abefore" && ! -e "$T/addon/trantor.lock" ]] || { echo "FAIL: the refused add changed the package"; exit 1; }
echo "ok: a package with no baseline to compose on refuses a change it cannot check"

# (10) A module that is not there, or that comes from two places, does not compose.
pbefore=$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")
if (cd "$T/mine" && "$TR" add org/ghost) > "$T/add11.out" 2>&1; then echo "FAIL: add of a package missing its exported module succeeded"; exit 1; fi
grep -q "exports \`Ghost\`, but there is no" "$T/add11.out" || { echo "FAIL: the missing module was not named"; cat "$T/add11.out"; exit 1; }
if (cd "$T/mine" && "$TR" add org/clash) > "$T/add12.out" 2>&1; then echo "FAIL: add of a package shipping the same module succeeded"; exit 1; fi
grep -q "two sources for the platform module \`Mine\`" "$T/add12.out" || { echo "FAIL: the clash was not named"; cat "$T/add12.out"; exit 1; }
[[ "$(sum "$T/mine/package.toml" "$T/mine/trantor.lock")" == "$pbefore" ]] || { echo "FAIL: a refused add changed the package"; exit 1; }
echo "ok: a missing exported module and a module from two places each fail the add"

# (11) A world variant in a subdirectory shares the lock: removing a dep from
# world.toml keeps the pin it uses, and world.toml's own platform is the one
# composed in target/ afterwards.
mkdir -p "$T/app/variants"
sed 's/^name = .*/name = "variant"/' "$T/app/world.toml" > "$T/app/variants/world.toml"
# Two links back to the project: variant discovery must not walk them forever.
mkdir -p "$T/app/docs" && ln -s . "$T/app/docs/a" && ln -s . "$T/app/docs/b"
(cd "$T/app" && perl -e 'alarm shift; exec @ARGV' 120 "$TR" remove greet) > "$T/rm2.out" 2>&1 || { echo "FAIL: remove with a variant (or it did not finish)"; cat "$T/rm2.out"; exit 1; }
grep -q "its pin stays" "$T/rm2.out" || { echo "FAIL: remove did not keep the pin a variant uses"; cat "$T/rm2.out"; exit 1; }
grep -q 'name = "greet"' "$T/app/trantor.lock" || { echo "FAIL: the variant's pin was dropped"; exit 1; }
! grep -q 'Greet' "$T/app/target/trantor/app/platform/main.roc" || { echo "FAIL: target/ holds a platform other than world.toml's"; exit 1; }
"$TR" compose "$T/app" --world variants/world.toml --out "$T/variant-out" > "$T/var.out" 2>&1 || { echo "FAIL: the variant no longer composes"; cat "$T/var.out"; exit 1; }
echo "ok: a variant in a subdirectory keeps its pin through a remove, and target/ is the edited world's"

# (12) A killed `new` is explained by the next one, which removes nothing
# (D-T3-22). A baseline whose driver.toml is a FIFO: composing it blocks, so
# the kill lands after new has written its files.
mkdir -p "$T/fifobase/components/drv" && cp "$PWD/tests/golden/u1-front-door/base/package.toml" "$T/fifobase/"
mkfifo "$T/fifobase/components/drv/driver.toml"
"$TR" new "$T/half" --from "$T/fifobase" > "$T/new1.out" 2>&1 & newer=$!
for _ in $(seq 100); do [[ -f "$T/half/.gitignore" ]] && break; sleep 0.1; done
sleep 0.5; kill -KILL "$newer"; wait "$newer" 2>/dev/null || true
[[ -f "$T/half/world.toml" && -f "$T/half/.trantor-new" ]] || { echo "FAIL: the killed new left no half project"; ls -la "$T/half"; exit 1; }
printf '# mine\n' >> "$T/half/Cargo.toml"
if "$TR" new "$T/half" --from "$CLI" > "$T/new2.out" 2>&1; then echo "FAIL: new ran over a killed new's half project"; exit 1; fi
grep -q "world.toml (as it wrote it)" "$T/new2.out" && grep -q "Cargo.toml (changed since)" "$T/new2.out" \
	|| { echo "FAIL: the refusal did not say what the killed new left"; cat "$T/new2.out"; exit 1; }
[[ -f "$T/half/world.toml" && -f "$T/half/Cargo.toml" && -f "$T/half/.trantor-new" ]] || { echo "FAIL: the refusal removed something"; ls -la "$T/half"; exit 1; }
rm -rf "$T/half" && "$TR" new "$T/half" --from "$CLI" > "$T/new3.out" 2>&1 || { echo "FAIL: new after the user cleared the half project"; tail -5 "$T/new3.out"; exit 1; }
echo "ok: a killed new is explained by the next one, file by file, and nothing is removed"

# A marker from elsewhere, with a path out of the project, is only explained:
# nothing outside the project is touched.
mkdir -p "$T/cloned" && printf 'keep\n' > "$T/victim.txt"
printf 'project\t/elsewhere\nfile\t../victim.txt\t0000000000000000\n' > "$T/cloned/.trantor-new"
if "$TR" new "$T/cloned" > "$T/new4.out" 2>&1; then echo "FAIL: new ran over a foreign marker"; exit 1; fi
grep -q "an interrupted \`trantor new\` left" "$T/new4.out" && [[ -f "$T/victim.txt" ]] || { echo "FAIL: a foreign marker was not explained, or something outside was touched"; cat "$T/new4.out"; exit 1; }
echo "ok: a foreign marker is explained and nothing outside the project is touched"

# (13) new without a baseline writes no app and says how to get one.
"$TR" new "$T/bare" > "$T/new.out" 2>&1
[[ ! -e "$T/bare/app/main.roc" ]] || { echo "FAIL: new with no baseline wrote an app"; exit 1; }
grep -q -- "--from" "$T/new.out" || { echo "FAIL: new with no baseline did not point at --from"; cat "$T/new.out"; exit 1; }
echo "ok: new with no baseline writes no app and points at --from"
echo "U1-ADD PASS"

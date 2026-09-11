#!/usr/bin/env bash
# U1 P1 acceptance: `trantor interface-stub` (D-U1-7, plan 2026-09-11).
#
# The claim is that the Rust signature for a hosted leaf is GENERATED rather
# than guessed, so this asserts the generated text against the one thing that
# can falsify it: a working host. Compiling is the weak half of the test — a
# `todo!()` body compiles and a missing `decref` compiles — so the gate also
# links, runs, and gauges, and proves the gauge itself is discriminating by
# removing the generated release and requiring the leak to show up.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=tests/golden/u1-stub
GEN="$FIX/target/trantor/u1-stub/abi/src/generated.rs"
SRC="$FIX/components/text-host/src/lib.rs"
cargo build --release -q

# The fixture ships the WORKING host; keep it aside so the world starts with no
# implementation at all, which is the state a new interface is really in.
REAL=$(mktemp); cp "$SRC" "$REAL"
restore() { cp "$REAL" "$SRC"; rm -f "$REAL"; }
trap restore EXIT
rm -f "$SRC"
# The composed tree is copied, never pruned, so a source file deleted here
# lingers under target/ and would build from the stale copy. (Worth knowing:
# deleting a component's source does not fail a build until the composed copy
# is cleared too.)
rm -rf "$FIX/target/trantor/u1-stub/components/text-host/src"

# (1) The documented prerequisite: glue is produced BEFORE cargo needs an impl,
# so the build fails at cargo and the glue is on disk regardless.
if ./target/release/trantor build "$FIX" --platform-only >/tmp/u1-stub-build.log 2>&1; then
	echo "FAIL: build succeeded with no host implementation — the fixture is not testing anything"; exit 1
fi
[[ -s "$GEN" ]] || { echo "FAIL: no glue at $GEN"; exit 1; }
echo "ok: glue on disk ($(wc -c <"$GEN" | tr -d ' ') bytes) though cargo has no impl yet"

# (2) Generate.
./target/release/trantor interface-stub "$FIX" text >/dev/null
[[ -s "$SRC" ]] || { echo "FAIL: interface-stub wrote nothing"; exit 1; }
n=$(grep -c 'extern "C-unwind" fn' "$SRC" || true)
[[ "$n" == 3 ]] || { echo "FAIL: $n generated signatures, want 3 (one per [[hosted]] leaf)"; exit 1; }
echo "ok: 3 signatures generated from 3 hosted leaves"

# (3) THE assertion: every generated signature is byte-identical to the one the
# working host uses. Anything else means the stub is a plausible guess.
if ! diff -u <(grep 'extern "C-unwind" fn' "$REAL") <(grep 'extern "C-unwind" fn' "$SRC"); then
	echo "FAIL: generated signatures differ from the working host's"; exit 1
fi
echo "ok: generated signatures are byte-identical to the working host's"

# (4) And so are the releases it emits — the owned-argument rule (B0) is a
# runtime property no compiler checks, so it is asserted textually here and
# behaviourally in (7).
if ! diff -u <(grep 'decref' "$REAL") <(grep 'decref' "$SRC"); then
	echo "FAIL: generated releases differ from the working host's"; exit 1
fi
echo "ok: generated releases match ($(grep -c decref "$SRC") decref call(s), and emit! correctly has none)"

# (5) The stub stands alone: it compiles unmodified against the real glue.
# Compose first so the newly written source reaches the composed workspace —
# the same order a user follows.
./target/release/trantor compose "$FIX" >/dev/null 2>&1
(cd "$FIX/target/trantor/u1-stub" && cargo build --release -q -p text-host) \
	|| { echo "FAIL: the generated stub does not compile"; exit 1; }
echo "ok: the generated stub compiles unmodified"

# (6) Fill in the bodies (restore the working host) and run.
restore; trap - EXIT
./target/release/trantor build "$FIX" --app app --out shout >/dev/null 2>&1 \
	|| { echo "FAIL: build with the working host"; exit 1; }
out=$("$FIX/target/trantor/u1-stub/bin/shout")
[[ "$out" == "58" ]] || { echo "FAIL: got '$out', want '58'"; exit 1; }
echo "ok: links and runs — Str in, Str out, U64 back"

# (7) Drop-balanced under the gauge, and the gauge is not vacuous.
g=$(TRANTOR_ALLOC_GAUGE=1 "$FIX/target/trantor/u1-stub/bin/shout" 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
[[ -n "$g" ]] || { echo "FAIL: no gauge line (driver not instrumented?)"; exit 1; }
allocs=$(sed -E 's/.*allocs=([0-9]+).*/\1/' <<<"$g"); live=$(sed -E 's/.*live=(-?[0-9]+).*/\1/' <<<"$g")
[[ "${allocs:-0}" -gt 0 ]] || { echo "FAIL: gauge saw no allocations, it is vacuous: $g"; exit 1; }
[[ "$live" == "0" ]] || { echo "FAIL: leaked with the generated releases in place: $g"; exit 1; }
echo "ok: drop-balanced ($g)"

# NEGATIVE CONTROL. Remove exactly what the stub generated and require the leak
# to appear: without this, (7) passes just as well on a gauge that never fails.
cp "$SRC" /tmp/u1-stub-real.rs
grep -v 'decref' /tmp/u1-stub-real.rs > "$SRC"
./target/release/trantor build "$FIX" --app app --out shout >/dev/null 2>&1 \
	|| { cp /tmp/u1-stub-real.rs "$SRC"; echo "FAIL: build without releases"; exit 1; }
bad=$(TRANTOR_ALLOC_GAUGE=1 "$FIX/target/trantor/u1-stub/bin/shout" 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
cp /tmp/u1-stub-real.rs "$SRC"; rm -f /tmp/u1-stub-real.rs
badlive=$(sed -E 's/.*live=(-?[0-9]+).*/\1/' <<<"$bad")
[[ "${badlive:-0}" -gt 0 ]] || { echo "FAIL: removing the generated releases did NOT leak ($bad) — the gauge proves nothing"; exit 1; }
echo "ok: removing the generated releases leaks ($bad) — the releases are load-bearing and the gauge is discriminating"

# Leave the fixture as it was found.
./target/release/trantor build "$FIX" --app app --out shout >/dev/null 2>&1
echo "U1 P1 interface-stub PASS"

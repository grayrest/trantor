#!/usr/bin/env bash
# SQ1 acceptance: the roc:sqlite-unsound generic-state fold over rusqlite.
# NON-RETAINING reducers (S7) over a seeded mixed-type table, each consuming the
# borrowed cells in place: an Integer aggregate, a Text concat (copy-on-consume),
# a Text predicate count, and a param-bound Real filter + count. All correct and
# drop-balanced on today's compiler. H0c: libsqlite3 is vendored only by
# rusqlite-host.
set -euo pipefail
cd "$(dirname "$0")/../../.."
S=tests/golden/sqlite-unsound
ROC="${ROC:-$HOME/.bin/roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }
cargo build --release -q

./target/release/hematite compose "$S" >/dev/null
( cd "$S" && ./build.sh app sq1 >/dev/null 2>&1 ) || { echo "FAIL: build sq1"; exit 1; }

rm -f /tmp/hematite-sq1.db
set +e; out=$(cd "$S" && ./bin/sq1 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: sq1 exit $code"; echo "$out"; exit 1; }
want=$'sum-id: 6\nnames: alice,amy,bob\na-names: 2\nhi-scores: 1'
[[ "$out" == "$want" ]] || { echo "FAIL: fold output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: generic-state fold over rusqlite — Integer aggregate, Text concat (copy), Text predicate count, param-bound Real filter"

# The fold drop-balances: row spines, boxes, reducer, result strings all freed;
# borrowed cells add no alloc/free.
rm -f /tmp/hematite-sq1.db
g=$(cd "$S" && HEMATITE_ALLOC_GAUGE=1 ./bin/sq1 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$g" || { echo "FAIL: fold leaked heap: $g"; exit 1; }
echo "ok: fold drop-balanced ($g)"

# H0c: libsqlite3 lives only in librusqlite_host.a.
T="$S/platform/targets/arm64mac"
{ ar t "$T/librusqlite_host.a" 2>/dev/null || true; } | grep -iE 'sqlite3|libsqlite' >/dev/null || { echo "FAIL: rusqlite-host bundles no libsqlite3"; exit 1; }
for a in "$T"/lib*.a; do
  [[ "$(basename "$a")" == "librusqlite_host.a" ]] && continue
  if { ar t "$a" 2>/dev/null || true; } | grep -iE 'sqlite3\.|libsqlite' >/dev/null; then echo "FAIL: $(basename "$a") also carries libsqlite3"; exit 1; fi
done
echo "ok: libsqlite3 vendored by rusqlite-host only (H0c)"
echo "SQ1 PASS"

# ---- SQ2: turso-host, the substitution proof ----
# The SAME fold app on the turso world (one wiring line swapped) must produce
# identical output. turso is pure Rust (turso_core, blocking); its cells are
# borrowed from owned Values held across the reducer call. Sole-vendor: each
# world's app links exactly one engine.
grep -q 'turso' "$S/components/turso-host/Cargo.toml" || { echo "FAIL: no turso-host"; exit 1; }
[[ "$(diff <(sed -n '4p' "$S/world.toml") <(sed -n '4p' "$S/world-turso.toml"))" ]] || true  # names differ
./target/release/hematite compose "$S" --world world-turso.toml >/dev/null
rm -f /tmp/hematite-sq1.db*
( cd "$S" && ./build.sh app sq-turso >/dev/null 2>&1 ) || { echo "FAIL: build turso world (framework link? R-SQ2)"; exit 1; }
rm -f /tmp/hematite-sq1.db*
set +e; tout=$(cd "$S" && ./bin/sq-turso 2>/dev/null); tcode=$?; set -e
[[ $tcode -eq 0 ]] || { echo "FAIL: sq-turso exit $tcode"; echo "$tout"; exit 1; }
[[ "$tout" == "$want" ]] || { echo "FAIL: turso output differs from rusqlite (substitution broken)"; diff <(echo "$want") <(echo "$tout") || true; exit 1; }
echo "ok: the same fold app runs identically on rusqlite and turso — substitution by one wiring line (H5)"

# turso drop-balances too.
rm -f /tmp/hematite-sq1.db*
tg=$(cd "$S" && HEMATITE_ALLOC_GAUGE=1 ./bin/sq-turso 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$tg" || { echo "FAIL: turso fold leaked heap: $tg"; exit 1; }
echo "ok: turso fold drop-balanced ($tg)"

# Sole-vendor: each app links exactly one engine (rusqlite XOR turso).
r_rus=$({ nm "$S/bin/sq1" 2>/dev/null || true; } | grep -icE 'rusqlite' || true)
r_tur=$({ nm "$S/bin/sq1" 2>/dev/null || true; } | grep -icE 'turso_core|turso_sdk' || true)
t_rus=$({ nm "$S/bin/sq-turso" 2>/dev/null || true; } | grep -icE 'rusqlite' || true)
t_tur=$({ nm "$S/bin/sq-turso" 2>/dev/null || true; } | grep -icE 'turso_core|turso_sdk' || true)
[[ "$r_rus" -gt 0 && "$r_tur" -eq 0 ]] || { echo "FAIL: rusqlite app should link rusqlite only (rusqlite=$r_rus turso=$r_tur)"; exit 1; }
[[ "$t_tur" -gt 0 && "$t_rus" -eq 0 ]] || { echo "FAIL: turso app should link turso only (rusqlite=$t_rus turso=$t_tur)"; exit 1; }
echo "ok: sole-vendor — rusqlite app links rusqlite ($r_rus)/turso(0); turso app links turso ($t_tur)/rusqlite(0)"

# leave the committed default (rusqlite) world composed + built.
./target/release/hematite compose "$S" >/dev/null
rm -f /tmp/hematite-sq1.db*
( cd "$S" && ./build.sh app sq1 >/dev/null 2>&1 )
echo "SQ2 PASS"

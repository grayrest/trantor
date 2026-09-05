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

# ---- SQ3: turso vector (base SQL) + encryption (host-side), no Roc leaf ----
# Vector search rides the base sql_fold! on turso; rusqlite rejects vector32.
# Encryption is host-side (key from env): correct reads, ciphertext at rest.
./target/release/hematite compose "$S" --world world-turso.toml >/dev/null
( cd "$S" && ./build.sh vector-app vec-turso >/dev/null 2>&1 ) || { echo "FAIL: build vector-app (turso)"; exit 1; }
vt=$(cd "$S" && ./bin/vec-turso 2>/dev/null || true)
[[ "$vt" == "vector: near,mid,far" ]] || { echo "FAIL: turso vector ordering (got: $vt)"; exit 1; }

KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
ENCDB=/tmp/hematite-sq3-enc.db
( cd "$S" && ./build.sh enc-app enc-turso >/dev/null 2>&1 ) || { echo "FAIL: build enc-app"; exit 1; }
# control: no key -> plaintext leaks into the file/wal (so the check isn't vacuous).
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
( cd "$S" && ./bin/enc-turso >/dev/null 2>&1 ) || true
plain=$(strings "$ENCDB" "$ENCDB"-wal 2>/dev/null | grep -c topsecret || true)
[[ "${plain:-0}" -gt 0 ]] || { echo "FAIL: unencrypted control shows no plaintext — the check is vacuous"; exit 1; }
# with key -> correct read, no plaintext at rest.
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
er=$(cd "$S" && HEMATITE_TURSO_ENCRYPTION_HEXKEY="$KEY" ./bin/enc-turso 2>/dev/null || true)
[[ "$er" == "enc-read: topsecret-alice" ]] || { echo "FAIL: encrypted read (got: $er)"; exit 1; }
cipher=$(strings "$ENCDB" "$ENCDB"-wal 2>/dev/null | grep -c topsecret || true)
[[ "${cipher:-0}" -eq 0 ]] || { echo "FAIL: plaintext leaked in the encrypted db/wal ($cipher)"; exit 1; }
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
echo "ok: turso encryption host-side — correct reads with the key, ciphertext at rest ($plain plaintext unencrypted vs 0 encrypted)"

# rusqlite rejects vector SQL (the negative half).
./target/release/hematite compose "$S" >/dev/null
( cd "$S" && ./build.sh vector-app vec-rusqlite >/dev/null 2>&1 ) || { echo "FAIL: build vector-app (rusqlite)"; exit 1; }
vr=$(cd "$S" && ./bin/vec-rusqlite 2>/dev/null || true)
[[ "$vr" == "vector: unsupported" ]] || { echo "FAIL: rusqlite should reject vector32 (got: $vr)"; exit 1; }
echo "ok: vector search runs on turso (near,mid,far), rejected by rusqlite — base SQL, no Roc leaf; encryption env-side"

# leave the default (rusqlite) world composed + built.
rm -f /tmp/hematite-sq1.db*
( cd "$S" && ./build.sh app sq1 >/dev/null 2>&1 )
echo "SQ3 PASS"

# ---- SQ4: roc:turso scalar UDF leaf (the turso superset) ----
# A Roc closure registered as a turso SQL scalar, invoked from a SELECT and a
# CREATE TRIGGER body. The rusqlite world does not expose roc:turso.
./target/release/hematite compose "$S" --world world-turso.toml >/dev/null
grep -q 'turso_register_scalar' "$S/interfaces/turso/Turso.roc" || { echo "FAIL: no turso_register_scalar leaf"; exit 1; }
( cd "$S" && ./build.sh udf-app udf-turso >/dev/null 2>&1 ) || { echo "FAIL: build udf-app (turso)"; exit 1; }
uo=$(cd "$S" && ./bin/udf-turso 2>/dev/null || true)
want_udf=$'triple-sum: 24\ntrigger-val: 30'
[[ "$uo" == "$want_udf" ]] || { echo "FAIL: Roc scalar from query/trigger (got: $uo)"; diff <(echo "$want_udf") <(echo "$uo") || true; exit 1; }
echo "ok: a Roc closure runs as a turso SQL scalar from a SELECT (24) and a TRIGGER (30)"

# The registered scalar is retained for the process by design: exactly one live
# allocation (the closure box), not a leak.
ug=$(cd "$S" && HEMATITE_ALLOC_GAUGE=1 ./bin/udf-turso 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=1' <<<"$ug" || { echo "FAIL: udf balance expected live=1 (the one registered closure): $ug"; exit 1; }
echo "ok: exactly the one registered scalar closure is retained ($ug)"

# The rusqlite world is not a turso superset: no Turso module, so udf-app can't build there.
./target/release/hematite compose "$S" >/dev/null
grep -q 'Turso' "$S/platform/main.roc" && { echo "FAIL: rusqlite world exposes Turso"; exit 1; } || true
( cd "$S" && ./build.sh udf-app udf-rusqlite >/dev/null 2>&1 ) && { echo "FAIL: udf-app built on rusqlite (roc:turso should be unavailable)"; exit 1; } || true
echo "ok: rusqlite world has no roc:turso — udf-app only builds on the turso superset"

# leave the default (rusqlite) world composed + built.
rm -f /tmp/hematite-sq1.db*
( cd "$S" && ./build.sh app sq1 >/dev/null 2>&1 )
echo "SQ4 PASS"

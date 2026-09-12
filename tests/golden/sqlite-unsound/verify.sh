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

if ! _b=$(./target/release/trantor build "$S" --app app --out sq1 2>&1); then echo "FAIL: build sq1" >&2; echo "$_b" >&2; exit 1; fi

rm -f /tmp/trantor-sq1.db
set +e; out=$(cd "$S" && ./target/trantor/sqlite-unsound/bin/sq1 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: sq1 exit $code"; echo "$out"; exit 1; }
want=$'sum-id: 6\nnames: alice,amy,bob\na-names: 2\nhi-scores: 1'
[[ "$out" == "$want" ]] || { echo "FAIL: fold output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: generic-state fold over rusqlite — Integer aggregate, Text concat (copy), Text predicate count, param-bound Real filter"

# The fold drop-balances: row spines, boxes, reducer, result strings all freed;
# borrowed cells add no alloc/free.
rm -f /tmp/trantor-sq1.db
g=$(cd "$S" && TRANTOR_ALLOC_GAUGE=1 ./target/trantor/sqlite-unsound/bin/sq1 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$g" || { echo "FAIL: fold leaked heap: $g"; exit 1; }
echo "ok: fold drop-balanced ($g)"

# H0c: libsqlite3 lives only in librusqlite_host.a.
T="$S/target/trantor/sqlite-unsound/platform/targets/arm64mac"
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
rm -f /tmp/trantor-sq1.db*
if ! _b=$(./target/release/trantor build "$S" --world world-turso.toml --app app --out sq-turso 2>&1); then echo "FAIL: build turso world (framework link? R-SQ2)" >&2; echo "$_b" >&2; exit 1; fi
rm -f /tmp/trantor-sq1.db*
set +e; tout=$(cd "$S" && ./target/trantor/sqlite-unsound/bin/sq-turso 2>/dev/null); tcode=$?; set -e
[[ $tcode -eq 0 ]] || { echo "FAIL: sq-turso exit $tcode"; echo "$tout"; exit 1; }
[[ "$tout" == "$want" ]] || { echo "FAIL: turso output differs from rusqlite (substitution broken)"; diff <(echo "$want") <(echo "$tout") || true; exit 1; }
echo "ok: the same fold app runs identically on rusqlite and turso — substitution by one wiring line (H5)"

# turso drop-balances too.
rm -f /tmp/trantor-sq1.db*
tg=$(cd "$S" && TRANTOR_ALLOC_GAUGE=1 ./target/trantor/sqlite-unsound/bin/sq-turso 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$tg" || { echo "FAIL: turso fold leaked heap: $tg"; exit 1; }
echo "ok: turso fold drop-balanced ($tg)"

# Sole-vendor: each app links exactly one engine (rusqlite XOR turso).
r_rus=$({ nm "$S/target/trantor/sqlite-unsound/bin/sq1" 2>/dev/null || true; } | grep -icE 'rusqlite' || true)
r_tur=$({ nm "$S/target/trantor/sqlite-unsound/bin/sq1" 2>/dev/null || true; } | grep -icE 'turso_core|turso_sdk' || true)
t_rus=$({ nm "$S/target/trantor/sqlite-unsound/bin/sq-turso" 2>/dev/null || true; } | grep -icE 'rusqlite' || true)
t_tur=$({ nm "$S/target/trantor/sqlite-unsound/bin/sq-turso" 2>/dev/null || true; } | grep -icE 'turso_core|turso_sdk' || true)
[[ "$r_rus" -gt 0 && "$r_tur" -eq 0 ]] || { echo "FAIL: rusqlite app should link rusqlite only (rusqlite=$r_rus turso=$r_tur)"; exit 1; }
[[ "$t_tur" -gt 0 && "$t_rus" -eq 0 ]] || { echo "FAIL: turso app should link turso only (rusqlite=$t_rus turso=$t_tur)"; exit 1; }
echo "ok: sole-vendor — rusqlite app links rusqlite ($r_rus)/turso(0); turso app links turso ($t_tur)/rusqlite(0)"

echo "SQ2 PASS"

# ---- SQ3: turso vector (base SQL) + encryption (host-side), no Roc leaf ----
# Vector search rides the base sql_fold! on turso; rusqlite rejects vector32.
# Encryption is host-side (key from env): correct reads, ciphertext at rest.
if ! _b=$(./target/release/trantor build "$S" --world world-turso.toml --app vector-app --out vec-turso 2>&1); then echo "FAIL: build vector-app (turso)" >&2; echo "$_b" >&2; exit 1; fi
vt=$(cd "$S" && ./target/trantor/sqlite-unsound/bin/vec-turso 2>/dev/null || true)
[[ "$vt" == "vector: near,mid,far" ]] || { echo "FAIL: turso vector ordering (got: $vt)"; exit 1; }

KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
ENCDB=/tmp/trantor-sq3-enc.db
if ! _b=$(./target/release/trantor build "$S" --world world-turso.toml --app enc-app --out enc-turso 2>&1); then echo "FAIL: build enc-app" >&2; echo "$_b" >&2; exit 1; fi
# control: no key -> plaintext leaks into the file/wal (so the check isn't vacuous).
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
( cd "$S" && ./target/trantor/sqlite-unsound/bin/enc-turso >/dev/null 2>&1 ) || true
plain=$(strings "$ENCDB" "$ENCDB"-wal 2>/dev/null | grep -c topsecret || true)
[[ "${plain:-0}" -gt 0 ]] || { echo "FAIL: unencrypted control shows no plaintext — the check is vacuous"; exit 1; }
# with key -> correct read, no plaintext at rest.
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
er=$(cd "$S" && TRANTOR_TURSO_ENCRYPTION_HEXKEY="$KEY" ./target/trantor/sqlite-unsound/bin/enc-turso 2>/dev/null || true)
[[ "$er" == "enc-read: topsecret-alice" ]] || { echo "FAIL: encrypted read (got: $er)"; exit 1; }
cipher=$(strings "$ENCDB" "$ENCDB"-wal 2>/dev/null | grep -c topsecret || true)
[[ "${cipher:-0}" -eq 0 ]] || { echo "FAIL: plaintext leaked in the encrypted db/wal ($cipher)"; exit 1; }
rm -f "$ENCDB" "$ENCDB"-wal "$ENCDB"-shm
echo "ok: turso encryption host-side — correct reads with the key, ciphertext at rest ($plain plaintext unencrypted vs 0 encrypted)"

# rusqlite rejects vector SQL (the negative half).
if ! _b=$(./target/release/trantor build "$S" --app vector-app --out vec-rusqlite 2>&1); then echo "FAIL: build vector-app (rusqlite)" >&2; echo "$_b" >&2; exit 1; fi
vr=$(cd "$S" && ./target/trantor/sqlite-unsound/bin/vec-rusqlite 2>/dev/null || true)
[[ "$vr" == "vector: unsupported" ]] || { echo "FAIL: rusqlite should reject vector32 (got: $vr)"; exit 1; }
echo "ok: vector search runs on turso (near,mid,far), rejected by rusqlite — base SQL, no Roc leaf; encryption env-side"

echo "SQ3 PASS"

# ---- SQ4: roc:turso scalar UDF leaf (the turso superset) ----
# A Roc closure registered as a turso SQL scalar, invoked from a SELECT and a
# CREATE TRIGGER body. The rusqlite world does not expose roc:turso.
grep -q 'turso_register_scalar' "$S/interfaces/turso/Turso.roc" || { echo "FAIL: no turso_register_scalar leaf"; exit 1; }
# The negative half runs FIRST. It needs the rusqlite world, which SQ3 has
# just left composed, so the turso half below costs one world switch instead
# of two — worth ~5s, because switching worlds recomposes the workspace with a
# different engine and cargo rebuilds it (2-5s against 0.6-1.0s for a
# same-world build). D-H7-38: both variants compose to one directory by
# design, so the switch cost is inherent and only ordering can avoid it.
#
# The rusqlite world is not a turso superset: no Turso module, so udf-app can't build there.
./target/release/trantor compose "$S" >/dev/null
grep -q 'Turso' "$S/target/trantor/sqlite-unsound/platform/main.roc" && { echo "FAIL: rusqlite world exposes Turso"; exit 1; } || true
if ./target/release/trantor build "$S" --app udf-app --out udf-rusqlite >/dev/null 2>&1; then echo "FAIL: udf-app built on rusqlite (roc:turso should be unavailable)"; exit 1; fi
echo "ok: rusqlite world has no roc:turso — udf-app only builds on the turso superset"

if ! _b=$(./target/release/trantor build "$S" --world world-turso.toml --app udf-app --out udf-turso 2>&1); then echo "FAIL: build udf-app (turso)" >&2; echo "$_b" >&2; exit 1; fi
uo=$(cd "$S" && ./target/trantor/sqlite-unsound/bin/udf-turso 2>/dev/null || true)
want_udf=$'triple-sum: 24\ntrigger-val: 30'
[[ "$uo" == "$want_udf" ]] || { echo "FAIL: Roc scalar from query/trigger (got: $uo)"; diff <(echo "$want_udf") <(echo "$uo") || true; exit 1; }
echo "ok: a Roc closure runs as a turso SQL scalar from a SELECT (24) and a TRIGGER (30)"

# The registered scalar is retained for the process by design: exactly one live
# allocation (the closure box), not a leak.
ug=$(cd "$S" && TRANTOR_ALLOC_GAUGE=1 ./target/trantor/sqlite-unsound/bin/udf-turso 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=1' <<<"$ug" || { echo "FAIL: udf balance expected live=1 (the one registered closure): $ug"; exit 1; }
echo "ok: exactly the one registered scalar closure is retained ($ug)"


echo "SQ4 PASS"

# ---- SQ5: clone-on-incref target (documented) + publish + tier ----
# The ergonomic record decode type-checks (the API shape supports it) but is NOT
# run — it retains borrowed cells, unsound until upstream clone-on-incref.
grep -q 'clone-on-incref' "$S/record-app/main.roc" || { echo "FAIL: record-app not marked the clone-on-incref target"; exit 1; }
cap "$ROC" check "$S/record-app/main.roc" >/dev/null 2>&1 || { echo "FAIL: record decode does not type-check (API shape broken)"; exit 1; }
echo "ok: record decode type-checks (API shape) — the documented clone-on-incref target, not run"

D="$S/target/trantor/sqlite-unsound/dist/platform/targets/arm64mac"

# Publish the TURSO world first, so the rusqlite publish below leaves the
# committed default composed and built as a side effect. Done the other way
# round this section needed a third build purely to restore that state.
if ! _b=$(./target/release/trantor build "$S" --world world-turso.toml --app app --out sq-turso 2>&1); then echo "FAIL: build sq-turso" >&2; echo "$_b" >&2; exit 1; fi
./target/release/trantor publish "$S" >/dev/null 2>&1
{ [[ -f "$D/libturso_host.a" ]] && [[ ! -f "$D/librusqlite_host.a" ]]; } || { echo "FAIL: turso baseline should ship turso, not rusqlite"; exit 1; }
[[ -f "$S/target/trantor/sqlite-unsound/dist/platform/Turso.roc" ]] || { echo "FAIL: turso baseline lacks the roc:turso module"; exit 1; }
echo "ok: turso baseline publishes (turso engine + Turso module, no rusqlite, no test scaffolding)"

# Publish the rusqlite world: engine + lock, no test scaffolding; Tier 2. This
# is also what leaves the committed default world composed + built.
rm -f /tmp/trantor-sq1.db*
if ! _b=$(./target/release/trantor build "$S" --app app --out sq1 2>&1); then echo "FAIL: build sq1 (default)" >&2; echo "$_b" >&2; exit 1; fi
./target/release/trantor publish "$S" >/dev/null 2>&1
{ [[ -f "$S/target/trantor/sqlite-unsound/dist/baseline.lock" ]] && grep -q abi_fingerprint "$S/target/trantor/sqlite-unsound/dist/baseline.lock"; } || { echo "FAIL: rusqlite publish produced no baseline.lock"; exit 1; }
[[ -f "$D/librusqlite_host.a" ]] || { echo "FAIL: rusqlite baseline lacks the engine archive"; exit 1; }
[[ ! -f "$D/libreport_host.a" ]] || { echo "FAIL: test-only report-host leaked into the baseline"; exit 1; }
# Captured, not piped (see b8-basic-cli/verify.sh): `tier` prints a count line
# after the verdict, and grep -q closing the pipe first makes trantor panic.
_t=$(./target/release/trantor tier "$S")
[[ "$_t" == Tier\ 2:* ]] || { echo "FAIL: sqlite world should tier as Tier 2 (host code) — got: $_t"; exit 1; }
echo "ok: rusqlite baseline publishes (engine + lock, no test scaffolding); Tier 2"

grep -q 'clone-on-incref' notes/2026-09-05-sqlite-unsound-design-log.md || { echo "FAIL: design log missing the clone-on-incref record"; exit 1; }

echo "SQ5 PASS"

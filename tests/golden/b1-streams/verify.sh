#!/usr/bin/env bash
# B1 acceptance: one InputStream resource over two backings (memory, file) via
# the sync-io Stream trait; stream resources drop-balance (live == 0); exit
# code == memory bytes.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b1-streams
cargo build --release -q
if ! _b=$(./target/release/hematite build "$B" --app app --out b1 2>&1); then echo "FAIL: build b1" >&2; echo "$_b" >&2; exit 1; fi
want_file=$(wc -c < "$B/world.toml" | tr -d ' ')
set +e
err=$(cd "$B" && ./target/hematite/b1-streams/bin/b1 2>&1 >/dev/null); code=$?
set -e
[[ $code -eq 11 ]] || { echo "FAIL: exit $code (expected 11 == memory bytes, or -1 on leak)"; echo "$err"; exit 1; }
grep -q "read 11 bytes" <<<"$err" || { echo "FAIL: memory stream read"; echo "$err"; exit 1; }
grep -q "read $want_file bytes" <<<"$err" || { echo "FAIL: file stream read (want $want_file)"; echo "$err"; exit 1; }
grep -q "live=0" <<<"$err" || { echo "FAIL: stream resources leaked"; echo "$err"; exit 1; }
echo "ok: memory(11) + file($want_file) through one InputStream; live=0; exit == 11"
echo "B1 PASS"

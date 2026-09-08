#!/usr/bin/env bash
# B4 acceptance: basic-cli's Utc/Sleep/Random/Locale/Url run byte-verbatim over
# roc:clocks / roc:random / roc:locale (+ pure Url), with exact output.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b4-small
C=/Users/grayrest/.cache/roc/packages/4rAQg8kUYZ3Vksr4qMQHpaFYNiHSn9GgS7gVxghd1XYV
cargo build --release -q
for m in Utc Sleep Random Locale Url InternalDateTime; do
  cmp -s "$C/$m.roc" "$B/components/basic-lib/$m.roc" || { echo "FAIL: $m.roc differs from basic-cli 0.21 (must be verbatim)"; exit 1; }
done
echo "ok: Utc/Sleep/Random/Locale/Url/InternalDateTime byte-identical to basic-cli 0.21"
./target/release/hematite compose "$B" >/dev/null
( cd "$B" && ./build.sh app b4 >/dev/null 2>&1 )
if ! _sc=$(./target/release/hematite scan "$B" 2>&1); then echo "FAIL: nm-scan (H0c symbol collision)" >&2; echo "$_sc" >&2; exit 1; fi
set +e; out=$(cd "$B" && LANG=en_US.UTF-8 B4_TAG=zh-Hant-TW ./bin/b4 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: exit $code"; echo "$out"; exit 1; }
want=$'utc-year-prefix: ok\nslept: ok\nrandom: ok\nlocale: ok\nlocales-listed: some\nlocale-parse: zh-Hant-TW\nurl-host: example.com'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: clocks/random/locale/url all correct, exit 0"
echo "B4 PASS"

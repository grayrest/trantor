#!/usr/bin/env bash
# B6 acceptance: basic-cli's Tcp/Http (byte-verbatim) plus new Udp sugar run over
# roc:sync-sockets + roc:sync-http. The peers are TEST-ONLY threads (roc:testnet).
# The httpd answers "hello-http" only to a GET, so the host's method encoding
# (basic-cli's to_host_method table, not union order) is observable.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b6-net
C=/Users/grayrest/.cache/roc/packages/4rAQg8kUYZ3Vksr4qMQHpaFYNiHSn9GgS7gVxghd1XYV
cargo build --release -q

for m in Tcp Http InternalHttp; do
  cmp -s "$C/$m.roc" "$B/components/net-lib/$m.roc" || { echo "FAIL: $m.roc not verbatim"; exit 1; }
done
echo "ok: Tcp.roc, Http.roc, InternalHttp.roc byte-verbatim from basic-cli 0.21"
grep -q '"CONNECT", "DELETE", "QUERY", "GET"' "$B/components/http-host/src/lib.rs" || { echo "FAIL: method table is not basic-cli's to_host_method encoding"; exit 1; }
grep -q 'packages' "$B/world.toml" || { echo "FAIL: world lacks [packages] (verbatim Http.roc needs import http.*)"; exit 1; }

./target/release/hematite compose "$B" >/dev/null
( cd "$B" && ./build.sh app b6 >/dev/null 2>&1 )
set +e; out=$(cd "$B" && ./bin/b6 2>/dev/null); code=$?; set -e
[[ $code -eq 0 ]] || { echo "FAIL: exit $code (nonzero = leaked socket resources or a failed step)"; echo "$out"; exit 1; }
want=$'tcp-echo: hi\nhttp-get: hello-http\nudp-echo: dgram\ntcp-accept: ping'
[[ "$out" == "$want" ]] || { echo "FAIL: output"; diff <(echo "$want") <(echo "$out") || true; exit 1; }
echo "ok: tcp connect+echo, http GET (verb-checked), udp bind/send/recv, tcp listen+accept; live=0"
echo "B6 PASS"

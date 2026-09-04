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

# ---- HC2: the streaming HTTP primitive over ureq ----
# http-stream-app drives HttpHost.send! + Streams.read! directly against the
# testnet: a large body arrives whole (streamed in chunks), a redirect chain is
# followed, both occurrences of a repeated header survive, a mid-body cutoff
# surfaces as StreamErr on read (H15), and a stall trips Timeout (H9). Exit code
# is the live resource count (0 = drop-balanced).
grep -q 'ureq' "$B/components/http-host/Cargo.toml" || { echo "FAIL: http-host is not built over ureq"; exit 1; }
grep -q 'body_stream' "$B/interfaces/sync-http/HttpHost.roc" || { echo "FAIL: HttpHost.Response is not a streaming body"; exit 1; }
( cd "$B" && ./build.sh http-stream-app b6-httpstream >/dev/null 2>&1 ) || { echo "FAIL: build http-stream-app"; exit 1; }
set +e; sout=$(cd "$B" && ./bin/b6-httpstream 2>/dev/null); scode=$?; set -e
[[ $scode -eq 0 ]] || { echo "FAIL: http-stream-app exit $scode (leaked stream/socket resources)"; echo "$sout"; exit 1; }
grep -qxF 'large: 100000' <<<"$sout" || { echo "FAIL: large body not streamed whole"; echo "$sout"; exit 1; }
grep -qxF 'redirect: 200 10' <<<"$sout" || { echo "FAIL: redirect not followed to /final"; echo "$sout"; exit 1; }
grep -qF  'x-multi|a|x-multi|b' <<<"$sout" || { echo "FAIL: repeated header not preserved in order"; echo "$sout"; exit 1; }
grep -qxF 'truncate: 10 errored' <<<"$sout" || { echo "FAIL: mid-body cutoff did not surface as StreamErr"; echo "$sout"; exit 1; }
grep -qxF 'stall: timeout' <<<"$sout" || { echo "FAIL: stall did not trip Timeout"; echo "$sout"; exit 1; }
echo "ok: streamed large body whole, followed redirect, preserved multi-value header, StreamErr on cutoff, Timeout on stall; live=0"

# H16: HEMATITE_HTTP_MAX_REDIRECTS=0 returns the 3xx unfollowed.
rout=$(cd "$B" && HEMATITE_HTTP_MAX_REDIRECTS=0 ./bin/b6-httpstream 2>/dev/null | grep '^redirect:' || true)
[[ "$rout" == "redirect: 302 0" ]] || { echo "FAIL: HEMATITE_HTTP_MAX_REDIRECTS=0 should return the 302 (got: $rout)"; exit 1; }
echo "ok: HEMATITE_HTTP_MAX_REDIRECTS=0 returns the 302 unfollowed"

# alloc-gauge: the streaming body paths drop-balance (heap-data leaks resource
# live() can't see).
g=$(cd "$B" && HEMATITE_ALLOC_GAUGE=1 ./bin/b6-httpstream 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
[[ -n "$g" ]] || { echo "FAIL: no alloc-gauge line from http-stream-app"; exit 1; }
grep -q 'live=0' <<<"$g" || { echo "FAIL: http streaming leaked heap allocations: $g"; exit 1; }
echo "ok: alloc-gauge drop-balanced across the http streaming paths ($g)"
echo "HC2 PASS"

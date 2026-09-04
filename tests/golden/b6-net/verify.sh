#!/usr/bin/env bash
# B6 acceptance: basic-cli's Tcp/Http (byte-verbatim) plus new Udp sugar run over
# roc:sync-sockets + roc:sync-http. The peers are TEST-ONLY threads (roc:testnet).
# The httpd answers "hello-http" only to a GET, so the host's method encoding
# (basic-cli's to_host_method table, not union order) is observable.
set -euo pipefail
cd "$(dirname "$0")/../../.."
B=tests/golden/b6-net
C=/Users/grayrest/.cache/roc/packages/4rAQg8kUYZ3Vksr4qMQHpaFYNiHSn9GgS7gVxghd1XYV
ROC="${ROC:-$HOME/.bin/roc}"
cap() { perl -e 'alarm shift; exec @ARGV' 120 "$@"; }
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

# ---- HC4: TLS, additive extra-CA trust, deterministic HTTPS ----
# Generate an EPHEMERAL cert via tools/local-cert (no committed key), trust it
# via HEMATITE_HTTP_EXTRA_CA (additive over webpki-roots, H13), and run an https
# GET against the local rustls testnet: a real handshake + cert validation +
# body-over-TLS, offline. The tls-OFF world (HC0's features knob) then rejects
# https and sheds the rustls stack; a world without sync-http links none (H0c).
command -v just >/dev/null 2>&1 || { echo "FAIL: HC4 needs 'just' for make-local-cert"; exit 1; }
CTMP=$(mktemp -d); trap 'rm -rf "$CTMP"' EXIT
CERT="$CTMP/cert.pem"
just make-local-cert "$CERT" >/dev/null 2>&1 || { echo "FAIL: just make-local-cert"; exit 1; }
[[ -f "$CERT" && -f "$CERT.key" ]] || { echo "FAIL: no cert/key generated"; exit 1; }

# default world = tls on.
./target/release/hematite compose "$B" >/dev/null
grep -q '^default = \["tls"\]' "$B/components/http-host/Cargo.toml" || { echo "FAIL: default world should compose http-host with tls"; exit 1; }
( cd "$B" && ./build.sh app b6 >/dev/null 2>&1 )
cap "$ROC" build --output="$B/bin/https-app" "$B/https-app/main.roc" >/dev/null 2>&1 || { echo "FAIL: build https-app"; exit 1; }
tls_on=$({ ar t "$B/platform/targets/arm64mac/libhttp_host.a" 2>/dev/null || true; } | grep -icE 'rustls|webpki' || true)
[[ "$tls_on" -gt 0 ]] || { echo "FAIL: tls-on http-host bundles no rustls (not self-contained)"; exit 1; }

# https GET with the extra CA trusted -> handshake + streamed body over TLS.
ho=$(cd "$B" && HEMATITE_TEST_CERT="$CERT" HEMATITE_HTTP_EXTRA_CA="$CERT" ./bin/https-app 2>/dev/null || true)
[[ "$ho" == "https: 200 https-hello" ]] || { echo "FAIL: https GET over TLS (got: $ho)"; exit 1; }
echo "ok: https:// over local rustls TLS with the extra CA trusted (handshake + cert validation + body-over-TLS)"

# Same cert, but NOT trusted -> validation fails (proves the trust is real, not blanket).
hu=$(cd "$B" && HEMATITE_TEST_CERT="$CERT" ./bin/https-app 2>/dev/null || true)
[[ "$hu" != "https: 200 https-hello" && "$hu" == https:* ]] || { echo "FAIL: untrusted cert should be rejected (got: $hu)"; exit 1; }
echo "ok: without HEMATITE_HTTP_EXTRA_CA the cert is rejected — trust is additive/opt-in, not blanket"

g=$(cd "$B" && HEMATITE_TEST_CERT="$CERT" HEMATITE_HTTP_EXTRA_CA="$CERT" HEMATITE_ALLOC_GAUGE=1 ./bin/https-app 2>&1 1>/dev/null | grep '^\[alloc-gauge\]' || true)
grep -q 'live=0' <<<"$g" || { echo "FAIL: https path leaked heap: $g"; exit 1; }
echo "ok: https streaming path drop-balances ($g)"

# tls-OFF world (HC0 knob): reject https with Other, shed rustls/webpki.
./target/release/hematite compose "$B" --world world-notls.toml >/dev/null
grep -q '^default = \[\]' "$B/components/http-host/Cargo.toml" || { echo "FAIL: world-notls should compose http-host with tls off"; exit 1; }
( cd "$B" && ./build.sh app b6 >/dev/null 2>&1 )
cap "$ROC" build --output="$B/bin/https-app" "$B/https-app/main.roc" >/dev/null 2>&1 || { echo "FAIL: build https-app (tls off)"; exit 1; }
hn=$(cd "$B" && HEMATITE_TEST_CERT="$CERT" HEMATITE_HTTP_EXTRA_CA="$CERT" ./bin/https-app 2>/dev/null || true)
[[ "$hn" == "https: other" ]] || { echo "FAIL: tls-off world should reject https with Other (got: $hn)"; exit 1; }
tls_off=$({ ar t "$B/platform/targets/arm64mac/libhttp_host.a" 2>/dev/null || true; } | grep -icE 'rustls|webpki' || true)
[[ "$tls_off" -eq 0 ]] || { echo "FAIL: tls-off http-host still bundles rustls ($tls_off members)"; exit 1; }
echo "ok: tls-off world rejects https:// with Other(msg) and sheds the crypto stack (rustls/webpki members: $tls_on on, $tls_off off)"

# H0c: a world WITHOUT sync-http links no ureq/rustls at all.
[[ -x tests/golden/b4-small/bin/b4 ]] || ( cd tests/golden/b4-small && ./build.sh app b4 >/dev/null 2>&1 )
b4syms=$({ nm tests/golden/b4-small/bin/b4 2>/dev/null || true; } | grep -c . || true)
[[ "$b4syms" -gt 100 ]] || { echo "FAIL: b4 has $b4syms symbols (stripped?) — the check would be vacuous"; exit 1; }
b4net=$({ nm tests/golden/b4-small/bin/b4 2>/dev/null || true; } | grep -icE 'ureq|rustls' || true)
[[ "$b4net" -eq 0 ]] || { echo "FAIL: an http-less world (b4) links ureq/rustls ($b4net symbols)"; exit 1; }
echo "ok: a world without sync-http links no ureq/rustls (H0c)"

# leave the committed default (tls-on) world composed + built.
./target/release/hematite compose "$B" >/dev/null
( cd "$B" && ./build.sh app b6 >/dev/null 2>&1 )
echo "HC4 PASS"

# ---- HC5: transparent decode over the stream (chunked / gzip / brotli) ----
# http-stream-app (run in HC2, output captured in $sout) also GETs /chunked,
# /gzip and /brotli. ureq de-chunks and decompresses transparently, so the
# stream yields the ORIGINAL text (Content-Length reflects the compressed size,
# harmless — apps read the stream, H15). The b8 migration RE-PROOF (baseline
# re-publish, Tier 1 unchanged, confinement swap, http examples running) is the
# b8 fixture's own verify, green after HC3/HC4.
grep -qxF 'chunked: hello-world' <<<"$sout" || { echo "FAIL: chunked not de-chunked"; echo "$sout"; exit 1; }
grep -qxF 'gzip: hello-gzip'     <<<"$sout" || { echo "FAIL: gzip not decompressed"; echo "$sout"; exit 1; }
grep -qxF 'brotli: hello-brotli' <<<"$sout" || { echo "FAIL: brotli not decompressed"; echo "$sout"; exit 1; }
echo "ok: chunked de-chunked, gzip + brotli decompressed transparently over the stream"
echo "HC5 (decode) PASS"

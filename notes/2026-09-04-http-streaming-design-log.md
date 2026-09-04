# `roc:sync-http` — full client (ureq) with a streaming body: design log

Session goal: replace the ~40-line plaintext `std::net` HTTP stub with a real
client (TLS, chunked, compression, redirects, timeouts). Grilling turned up a
pivot mid-session — from "keep the interface frozen" to a **streaming response
body** — so this log supersedes the frozen-interface framing for HTTP. Decisions
are H1–H15; they extend, and where noted revise, the basic-cli-port decisions
(P10 shared types, P12 sync, P13 opt-in weight, H0c sole-vendor).

## The pivot

The stub collected the whole body into `List(U8)`. Following WASI's
`incoming-response → incoming-body → input-stream`, the response body becomes a
**`roc:sync-io` `InputStream`** — the same refcounted resource files, sockets,
and stdin already produce (`sync_io_core::input_stream(Box<dyn Read>)`). `send!`
returns status + headers as soon as the final headers arrive; the app reads the
body stream, or collects it if it wants the whole thing. This changes the
response type, so "keep the interface unchanged" no longer holds for HTTP — it
was the right trade for the better design.

## Decisions

**H1 — Client: `ureq`, not `hyper`/`tokio`.** ureq is natively blocking — it
*is* the P12/`sync-http` shape, no runtime — and rustls-based (no OpenSSL). One
well-scoped vendored crate (H0c), far lighter than basic-cli's
`hyper + hyper-rustls + tokio` async stack run under `block_on`. The cost is not
being basic-cli's exact bytes; the interface hides that.

**H2 — ureq `=3.4.0`, `default-features = false, features = ["rustls", "gzip",
"brotli"]`.** `rustls` pulls ring + webpki-roots — the same crypto provider and
bundled Mozilla root store basic-cli chose (`hyper-rustls … webpki-tokio, ring`).
gzip **and** brotli decompress transparently (a divergence from basic-cli, which
hands back raw bytes — but transparent to the interface and more useful).
Chunked transfer-encoding is decoded internally by ureq (no feature). Exact pin
keeps the ABI fingerprint stable (H0c). Off: native-tls, cookies, json (Roc owns
JSON), charset, multipart, socks.

**H3 — Error mapping onto the 4-variant twin `BadBody | NetworkError | Other |
Timeout`.** `Timeout → Timeout`; `Io`/`ConnectionFailed`/`Tls`/DNS/connect →
`NetworkError`; body-read / decompression / oversize / malformed-response →
`BadBody`; everything else → `Other(msg)`. Non-2xx is **success** (a `Response`
with that status), not an error — only transport failures become `TransportErr`.

**H4 — Follow redirects (ureq default, up to 10, browser-like).** 303 → GET,
307/308 preserve method; the returned `Response` and its body stream are always
the final hop's. Diverges from basic-cli (hyper's legacy client doesn't follow),
accepted for the fuller client. Per-request redirect policy is a future
interface field, not this pass.

**H5 — Streaming response body (the pivot).** `HttpHost.send!` returns
`{ status: U16, headers_flat, body_stream: InputStream }` instead of a collected
`List(U8)`. The host mints the stream from ureq's `into_reader()`
(`BodyReader<'static>`, owns the connection) via `sync_io_core::input_stream`.
The HTTP body joins files/sockets/stdin as an `InputStream` producer.

**H6 — App-facing shape: streaming default + collect helper + bridge (option
C).** Hematite `Response { status, headers, body : Streams.InputStream }` is the
default. `Http.read_body_to_end!(response) => List(U8)` is the "I want the whole
thing" collect. `Http.to_http_response!(response) =>` `roc-lang/http` `Response`
bridges to the eager shared type for interop. **Requests keep** `roc-lang/http`
`Request`/`Method`/`Header` (P10 holds on the request side). `roc-lang/http`'s
`Response` (`body : List(U8)`) is no longer used for the live response — the two
http examples take a one-line collect adaptation, the deliberate migration cost
of choosing streaming.

**H7 — Request body stays `List(U8)`.** Only responses stream; the app builds
the request body whole (`with_body`/`with_json_body`), ureq takes the bytes.
Request-body streaming (WASI `output-stream` symmetry, for huge uploads) is a
future addition behind the same `OutputStream` primitive.

**H8 — One process-wide lazy `Agent`.** Built once behind a `OnceLock`
(redirect limit, rustls TLS, gzip/brotli), reused for every `send!` → keep-alive
+ connection pooling, TLS initialized once. A pooled connection returns to the
pool when the body stream is drained or dropped. Per-call `timeout_ms` is a
per-request config override, not an agent rebuild. `Agent` is `Send + Sync`
(internally `Arc`); the model is single-threaded anyway (P12).

**H9 — Timeout: per-phase anti-stall.** `timeout_ms` bounds
connect + request + response-headers for `send!`, **and** is set as a
receive/between-chunks timeout on the body `InputStream`: a *stalled* read fails,
but a steadily-flowing large download is not killed by a wall-clock deadline.
This is WASI's first-byte/between-bytes flavor mapped onto the single knob.
`0` = no timeout anywhere. (Divergence from basic-cli's one overall deadline
around the whole collect.)

> **H9 revised at HC2 (measured against ureq 3.4.0).** ureq 3.4.0's
> `ConfigBuilder` exposes no between-bytes / per-read timeout — the body-phase
> options are `timeout_recv_response` (headers only) and `timeout_recv_body` (a
> *total* budget, explicitly "not restarted for each read"). A true stall guard
> (SO_RCVTIMEO) would require a bespoke `Connector` chain, out of scope for this
> pass. So `timeout_ms` maps to `timeout_connect` + `timeout_recv_response` +
> `timeout_recv_body`, all equal: a stall trips `Timeout` (termination
> guaranteed), but a very large/slow body is *also* bounded by the same deadline
> — i.e. the body reverts to basic-cli's one-overall-deadline rather than H9's
> "don't kill steady large downloads." Decided with the user at HC2; a
> between-bytes guard via a custom connector remains a clean future addition.
> `0` still means no timeout anywhere.

**H10 — Response headers: NUL-joined, every occurrence preserved.** R-B5 still
holds (pinned glue can't build `RocList<RocStr>` host-side), so headers cross as
`name\0value\0…` and the shim splits into `List (Str, Str)`. Every occurrence is
emitted in order (correct for repeated `Set-Cookie`; no coalescing). NUL is safe
(HTTP forbids it in names/values; rustls/ureq reject control chars).

**H11 — Trailers dropped this pass.** ureq 3.4 does not expose HTTP trailers at
all (verified: zero "trailer" in the crate; its chunked decoder discards them).
Exposing them would require going back to hyper. Deferred as a future
`Response.finish! => List Header` gated on a ureq feature or a client swap.

**H12 — http in the baseline; TLS behind a default-on `tls` feature.** The
`roc:basic-cli` all-in-one keeps http (parity, the http examples migrate), and
`http-host` is the sole vendor of ureq/rustls/ring (H0c — a world without
`sync-http` links none, verified with `nm`/`ar`). The heavy TLS stack sits behind
a **`tls` Cargo feature on `http-host`, default on**: off → ureq with no TLS
provider (plaintext `http://` only, sheds rustls/ring/webpki-roots), and an
`https://` request returns `Other("https requires the tls feature, which is
disabled in this build")`. This needs a **new general per-component Cargo-features
knob in the composer** (a manifest field flowing into the generated `Cargo.toml`,
useful beyond TLS; default = the crate's own defaults).

**H13 — Trust: webpki-roots + additive `HEMATITE_HTTP_EXTRA_CA`.** With `tls`
on, the rustls root store is webpki-roots **plus** any certs at the
`HEMATITE_HTTP_EXTRA_CA` PEM path (additive — public CAs still trusted; opt-in;
`NODE_EXTRA_CA_CERTS`-style). One env var, one PEM file (may hold several certs),
no directory scan, no replace-the-store mode. It is both the deterministic
local-TLS **test** hook and the **easy local-cert route** for any hematite user
(local dev cert, corporate CA, self-signed service) — same path the suite
exercises, so it can't rot. Ignored when `tls` is off.

**H14 — Testing: deterministic, offline, incl. TLS.** `testnet` (test-only host
threads) grows to serve, over plaintext, a streamed large body, chunked
transfer-encoding, gzip+brotli, a 301/302/307/308 redirect chain, a stalled
server for the timeout, and multi-value headers. TLS is exercised deterministically:
`testnet` runs a rustls server with an **ephemeral** cert, the test build points
`HEMATITE_HTTP_EXTRA_CA` at it, and an `https://` GET proves handshake + cert
validation + streaming-a-body-over-TLS. Cert generation is a small self-contained
**`tools/local-cert` crate (rcgen)**, wrapped by **`just make-local-cert
<cert-path>`** (writes the cert PEM to `<cert-path>`, the key to
`<cert-path>.key`, SANs `localhost`/`127.0.0.1`/`::1`, long validity, prints the
`export HEMATITE_HTTP_EXTRA_CA=…` line). The suite generates a throwaway cert per
run via the same tool — **no committed cert or key** (keeps a private key out of
git, dodges expiry).

**H15 — Body-phase failures surface through the stream.** `send!` reports only
connect/header-phase failures; a failure while the body streams (reset,
truncation, decompression error) surfaces as `StreamErr(IOErr)` on the
`InputStream` read. WASI-consistent (the `input-stream` owns its errors); a shift
from basic-cli, where the collect made mid-body failures a send-time error.
Accepted — the only alternative is re-collecting in `send!`, which defeats
streaming. Minor consequences noted: gzip/brotli means the stream yields
decompressed bytes while `Content-Length` reflects the compressed size (apps read
the stream, harmless); redirects resolve inside `send!` (final hop only);
early-drop of the stream closes/recycles the connection via the B0 destructor;
the `to_host_method` u8 table + `method_ext` for `Unknown` carries over unchanged.

**H16 — Global redirect knob: `HEMATITE_HTTP_MAX_REDIRECTS`.** The shared
Agent's `max_redirects` is read from this env when the Agent is built — default
`10` (H4's browser-like following), `0` = don't follow (a 3xx comes back as the
`Response`, restoring basic-cli's behavior without a rebuild). Per-process,
runtime, no interface change; consistent with the `HEMATITE_HTTP_EXTRA_CA`
env convention (H13). `max_redirects_will_error` stays **false** (hitting the
ceiling returns the last response, not an error — browser-like);
`redirect_auth_headers` keeps ureq's default. **Per-request** redirect policy
remains a future interface field: the shared `roc-lang/http` `Request` has no
redirect field, so it cannot be per-request without extending that type.

## Interface change summary (vs the frozen stub)

- Primitive `roc:sync-http/HttpHost.send!`: response `body : List(U8)` →
  `body_stream : Streams.InputStream`. `roc:sync-http` now depends on
  `roc:sync-io` (like sockets). Status + `headers_flat` unchanged.
- App-facing `Http`: `send! => Try(Response, TransportErr)` with a streaming
  `Response`; new `read_body_to_end!` and `to_http_response!`. `Http.roc` /
  `InternalHttp.roc` are no longer byte-verbatim from basic-cli.
- Composer: new per-component `features` knob (H12).
- New env: `HEMATITE_HTTP_EXTRA_CA` (H13), `HEMATITE_HTTP_MAX_REDIRECTS` (H16).
  New Cargo feature: `http-host/tls` (default on, H12). New crate:
  `tools/local-cert` + `just make-local-cert` (H14).

## Non-goals this pass

Trailers (H11), request-body streaming (H7), per-request redirect/TLS policy
fields, HTTP/2, cookies, a replace-the-store TLS mode, proxies. All are clean
future additions behind the shapes chosen here.

## HC3 implementation notes (app-facing streaming Http in b8)

- **`Response` name clash.** Http's local streaming `Response` type shadows the
  `http.Response` module, so `Response.from_status` can't be called from Http.
  The bridge builder (`to_http_response`) therefore lives in `InternalHttp`
  (which has no local `Response`), and `Http.to_http_response!` forwards to it;
  its return type is `InternalHttp.HttpResponse` (an alias for the http package's
  `Response`). This Roc has no `import ... as` aliasing to resolve it otherwise.
- **`decode_json_response!` gained a `!`.** It now reads the body stream, so it
  is effectful; the http-client example's call gains the `!` — one of the
  streaming adaptation lines.
- **In-process testnet (user's choice at HC3).** The examples are standalone
  binaries with hardcoded `:9000` URLs and start no server, so each adapts with
  a `TestNet.start_test_server!({})` line (+ its import) plus the streaming
  accessor edit — a handful of lines, not a pure URL swap. `testnet-host` serves
  basic-cli's ci endpoints on :9000.
- **`test_only` composer flag.** `testnet-host` is linked into apps but must not
  ship in the baseline; a new `[components.X] test_only = true` manifest flag
  makes `publish` omit that archive from `dist/` (b8's long-standing "no test
  scaffolding in the baseline" guard now bites for real).
- **Migration cost, measured.** Streaming drops Http + InternalHttp from the
  byte-verbatim set and moves the 2 http examples from pure-URL-swap to
  URL-swap + a few streaming lines; the other 26 non-sqlite examples still
  migrate by URL alone, and the 2 http examples now RUN (they only checked
  before).

## Open items

1. **`get_utf8!` / http examples adaptation** — the exact minimal edit
   (`response.body()` → `Http.read_body_to_end!(response)` then decode); confirm
   it stays one line.
2. **ureq rustls provider install** — whether ureq 3.4's `_ring` wires the
   default `CryptoProvider` automatically or the host must
   `install_default()` once; settle at implementation (context7/ureq docs).
3. **Gate breakdown** — fold H1–H15 into the plan as a gate (compose feature
   knob → host over ureq streaming → TLS + extra-CA → testnet growth + local-cert
   tool → migration re-proof), the way B0–B8 were structured.

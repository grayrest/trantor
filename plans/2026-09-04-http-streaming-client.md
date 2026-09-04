# Plan: `roc:sync-http` full streaming client (ureq)

> **Status: COMPLETE** (HC0–HC5 all landed, full golden suite green). Two
> plan-time forks were settled with the user during implementation — the
> features knob rewrites the host crate's own `[features] default` (not a
> driver→host edge), and the http examples run against an in-process testnet
> (so they take a few streaming-adaptation lines, not a pure URL swap). Two
> measured revisions are recorded in the design log: H9's body timeout is a
> total budget (ureq 3.4 has no between-bytes timeout), and H13's additive
> store is built via the `webpki-root-certs` crate. A `test_only` composer flag
> was added so testnet-host links into apps but stays out of the baseline.

Implements the [HTTP streaming design log](../notes/2026-09-04-http-streaming-design-log.md)
(decisions H1–H16): replace the plaintext `std::net` stub with ureq, streaming
the response body as a `roc:sync-io` `InputStream`, with TLS, redirects,
compression, per-phase timeouts, and env knobs.

Gates HC0–HC5. Each ends with a self-checking `verify.sh`; commits are gated on
the full suite (currently 13/13) staying green plus the gate's own checks.
Fixtures under `tests/golden/` build on the B6/B8 net stack.

Dependency order: HC0 and HC1 are independent prerequisites; HC2 → HC3 → HC5 is
the client spine; HC4 depends on HC0 (features), HC1 (cert), and HC2/HC3.

## HC0 — Composer: per-component Cargo-features knob (H12 mechanism)

- `manifest.rs`: `[components.<name>]` gains `features: Vec<String>` and
  `default_features: Option<bool>` (default = the crate's own defaults).
- `codegen.rs`: emit `features`/`default-features` into the generated component
  `Cargo.toml`, and into the workspace member's dependency line where the driver
  depends on it. HTTP-independent; the general knob the `tls` feature rides.
- **Exit:** a golden fixture composes one component two ways — with and without a
  named feature — and the generated `Cargo.toml` reflects each; both build; a
  symbol/`nm` check confirms the feature actually changed the output (a gated
  symbol present/absent). `verify.sh` asserts the manifest→Cargo.toml mapping and
  the build divergence.

## HC1 — `tools/local-cert` crate + `just make-local-cert` (H14 tooling)

- New `tools/local-cert` crate (rcgen 0.14): generate a self-signed cert with
  SANs `localhost`/`127.0.0.1`/`::1`, long validity; write the cert PEM to the
  given path and the key to `<path>.key`.
- New `justfile` with `make-local-cert <cert-path>`: check the toolchain (cargo),
  `cargo run -p local-cert -- <cert-path>`, print the `export
  HEMATITE_HTTP_EXTRA_CA=<cert-path>` line.
- **Exit:** `just make-local-cert <tmp>` produces a cert+key that rustls/openssl
  parse and that chains to itself for `localhost`; a unit test in the crate loads
  the generated PEM into a rustls `RootCertStore`. Standalone, no HTTP.

## HC2 — Streaming primitive over ureq, plaintext (H1–H5, H7–H10, H15, H16)

- `roc:sync-http` interface: `HttpHost.send!` return record body field becomes
  `body_stream : Streams.InputStream` (status + `headers_flat` unchanged);
  `roc:sync-http` now imports `roc:sync-io`.
- `http-host` rewritten over ureq (`=3.4.0`, `default-features = false`,
  `features = ["gzip", "brotli"]` here — `tls` added in HC4): one process-wide
  lazy `Agent` (H8); `send!` returns status + NUL-joined multi-value headers
  (H10) + a body `InputStream` minted from `resp.into_body().into_reader()` via
  `sync_io_core::input_stream` (H5); request body `List(U8)` (H7); error mapping
  to the 4-variant twin (H3); redirects from `HEMATITE_HTTP_MAX_REDIRECTS`
  (default 10, `max_redirects_will_error=false`, H16); per-phase anti-stall
  `timeout_ms` on the call and the body stream (H9); `to_host_method` u8 map
  unchanged (H15).
- `testnet` (test-only threads): a plaintext httpd that streams a large body,
  chunked, follows a redirect chain, stalls for the timeout, and repeats a
  header.
- **Exit:** over the plaintext testnet an app `send!`s, reads the body stream in
  chunks, and collects; a large body arrives whole; a redirect chain is followed
  and `HEMATITE_HTTP_MAX_REDIRECTS=0` returns the 3xx; a stalled server trips the
  timeout as `Timeout`; a mid-body cutoff surfaces as `StreamErr` on read (H15);
  multi-value headers arrive as separate pairs; resource `live()==0` and the
  alloc-gauge reports `live=0`.

## HC3 — App-facing streaming `Http` + migration adaptation (H6)

- `Http.roc`: `Response { status, headers, body : Streams.InputStream }`;
  `send! => Try(Response, TransportErr)`; `read_body_to_end!(response) => List
  U8`; `to_http_response!(response) =>` `roc-lang/http` `Response` bridge;
  `get_utf8!`/`get!` re-expressed over `read_body_to_end!`. `InternalHttp` crossing
  updated to the streaming record. Requests keep `roc-lang/http`
  `Request`/`Method`/`Header` (P10 holds on requests).
- **Exit:** `http-client` and `http-simple` build and run against the plaintext
  testnet after the one-line `response.body()` → `read_body_to_end!` collect;
  `get_utf8!` returns the body; `to_http_response!` round-trips into the shared
  eager `Response`. `verify.sh` bounds the example edits at one line each.

## HC4 — TLS, extra-CA trust, deterministic HTTPS (H12, H13, H14)

- `http-host`: add the `tls` Cargo feature (default on) → `ureq/rustls`
  (ring + webpki-roots); build the root store from webpki-roots **plus** any
  certs at `HEMATITE_HTTP_EXTRA_CA` (additive, H13); `https://` with `tls` off →
  `Other("https requires the tls feature…")`.
- `testnet`: a rustls server presenting HC1's cert. `verify.sh` generates an
  **ephemeral** cert via `tools/local-cert` into a temp dir, points
  `HEMATITE_HTTP_EXTRA_CA` at it, runs an `https://` GET.
- Compose a `tls`-off `http-host` (via HC0's features knob) for the negative case.
- **Exit:** the `https://` GET against the local rustls testnet succeeds
  (handshake + cert validation + streaming-a-body-over-TLS), deterministic and
  offline, no committed key; the `tls`-off world returns `Other(msg)` for
  `https://` and `nm`/`ar` show no rustls/ring symbols; H0c — a world without
  `sync-http` links no ureq/rustls (`ar t` across archives).

## HC5 — Decode coverage + baseline re-publish + migration re-proof (H2, H6, H14)

- `testnet` serves `chunked`, `gzip`, and `brotli` bodies; the stream yields
  decoded bytes (Content-Length vs stream-length disagreement noted, H15).
- Re-publish the `roc:basic-cli` baseline with the streaming `Http`; re-run the
  B8 migration (http examples now **run**, others unchanged), `publish`, `tier`
  (pure-Roc still Tier 1), and the confinement swap; alloc-gauge + resource
  balance across the http paths.
- **Exit:** chunked/gzip/brotli decode correctly over the stream; the full B8
  suite stays green with the http examples running; the baseline re-publishes and
  tiers unchanged.

## Risks

- **R-HC1 — rustls `CryptoProvider` install.** Whether ureq 3.4's `_ring` wires
  the process-default provider automatically or `http-host` must
  `CryptoProvider::install_default()` once at Agent build. Settle at HC4
  (context7/ureq docs); a missing install fails every TLS call at runtime.
- **R-HC2 — streaming body reader lifetime.** `into_reader()` returns
  `BodyReader<'static>` that owns the connection; confirm it composes with the
  shared pooled `Agent` and that the B0 `InputStream` destructor returns/closes
  the connection on early drop (HC2 gauge).
- **R-HC3 — per-phase timeout on the body stream.** Confirm ureq exposes a
  recv-body / between-bytes timeout settable when minting the reader, so H9's
  anti-stall applies to the stream and not just `send!` (HC2).
- **R-HC4 — features knob vs the shared workspace / ABI fingerprint.** A
  `tls`-off component must not pull rustls transitively through the workspace;
  the per-component `features` must not leak across members (HC0/HC4 `nm`).
- **R-HC5 — gzip/brotli + streaming length.** The stream yields decoded bytes
  while `Content-Length` reflects compressed size; ensure no verify assertion
  ties body length to the header (HC5).

## Non-goals (deferred, clean future additions)

Trailers (H11), request-body streaming (H7), per-request redirect/TLS policy
fields, HTTP/2, cookies, a replace-the-store TLS mode, proxies.

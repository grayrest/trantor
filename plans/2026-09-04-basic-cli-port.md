# Plan: basic-cli → hematite port

**Design log:** [`notes/2026-09-04-basic-cli-port-design-log.md`](../notes/2026-09-04-basic-cli-port-design-log.md) (P1–P14, all resolved).
**hematite design log:** [`notes/2026-09-04-hematite-design-log.md`](../notes/2026-09-04-hematite-design-log.md) (D1–D23) — `Dn` references below.

**Definition of done:** basic-cli's 0.21 surface (20 modules, ~55 hosted
symbols, minus sqlite) decomposed into `roc:*` interfaces grouped à la WASI
(P1/P2/P6), each package implemented in a crate (P9), composed into an
all-in-one `roc:basic-cli` world (P14) that an existing basic-cli app ports
onto **by changing only its platform URL**. Filesystem is capability-based
(descriptors/preopens, P4) with basic-cli's path API as derived sugar; resources
are refcounted opaque handles (P5); I/O is synchronous blocking (P12).
seahaven's confinement becomes a swappable `roc:filesystem` implementation, not
a fork (P14).

**Prime directive:** B0 is a hematite *feature* (the resource model) and is the
load-bearing prerequisite — nothing that returns a handle can be built until
its drop-balance is proven. Gates after B0 each port one WASI package and each
ends with basic-cli app code running **unchanged** over it; that is the only
exit criterion that measures the migration promise.

---

## Architecture at a glance

```
app (basic-cli code, unchanged)          derived layer (pure Roc, D2)
────────────────────────────────         ──────────────────────────────
import pf.File / pf.Path / pf.Stdout ──► File.roc  Path.roc  Stdout.roc …
                                             │  open-at → read-via-stream → drop
                                             ▼
primitive interfaces (bodiless, WASI-shaped, P1/P2/P6)
  roc:cli/{environment,exit,stdin,stdout,stderr,terminal-*}
  roc:filesystem/{types,preopens}      roc:sync-io/streams   roc:io/error
  roc:clocks/*  roc:random/random      roc:sync-sockets/*    roc:sync-http/*
  roc:subprocess   roc:locale   roc:temporal
                                             │
host crates, one per package (P9)  ──────────┴──► resources: refcounted Box(U64)
  + shared substrate: sync-io (Stream trait), io/error        + dealloc-registry destructor (B0)
```

Drivers: `run!` (WASI-shaped, P8) and the `main!`-compat driver (basic-cli
apps unchanged). Path packages `roc:path` (Str) and `roc:os-path` (lossless)
both ship; a world picks by D14 rename (P11).

---

## Gates

### B0 — Resource model ⚠ load-bearing prerequisite, runs first

The glue's `RocBoxPayloadDecref` destructor is invoked **only** by host-side
`decref_box_with`/`free_box_with`. When *Roc* drops the last reference its
runtime calls plain `roc_dealloc` — **no destructor hook.** So P5's
"destructor fires on the last Roc drop" needs a native mechanism: the host owns
`roc_dealloc` (one of the six driver runtime symbols), so it can consult a
**dealloc registry** before freeing.

- **Mechanism** (`hematite_abi::resource`, in the generated `abi` wrapper):
  `resource_new(value, destructor) -> RocBox` allocates via `allocate_box`
  (8-byte payload, align 8, non-refcounted → `header_bytes = 8`), boxes the
  Rust value behind a raw pointer in the payload, and registers
  `base = data - 8 → destructor` in a global registry. The generated driver's
  `roc_dealloc(ptr, align)` looks `ptr` up: if registered, run the destructor,
  unregister, then default-dealloc. `resource_get(box) -> &T` borrows.
- **IDL:** `interface.toml` gains `[[resources]]` (`name = "Counter"`); the
  binding module declares `Counter :: Box(U64)` (basic-cli's exact spelling);
  hosted leaves may take/return it.
- **Drop-balance spike** (`spikes/b0-resource/`): a `gauge` host component
  exporting a `Counter` resource — `open! : {} => Counter`, `bump! : Counter =>
  U64` — with host `OPENS`/`CLOSES` counters incremented on `resource_new` and
  in the destructor. The app opens N, bumps each, passes one to a helper (a
  refcount bump = borrow), lets them all fall out of scope. At exit the host
  asserts **`CLOSES == OPENS == N`** and that the borrowed one closed exactly
  once (not on the borrow).
- **NO-GO handling:** if Roc's runtime passes something other than the
  allocation base to `roc_dealloc` (registry miss), key the registry on the
  data pointer and reconcile with `header_bytes`; if Roc frees boxes through a
  path that bypasses `roc_dealloc` entirely, P5 needs an explicit `close!` and
  the design log gets a dated amendment before B1.
- **Exit:** the gauge balances; `hematite compose` emits `Box(U64)` resource
  aliases from `[[resources]]`; the driver's generated `roc_dealloc` consults
  the registry.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b0-resource-model.md)):
> `opens=3 closes=3 live=0`, exit 3; every `roc_dealloc` a registry HIT at
> `data − 8` (**R-B1 resolved** — Roc passes the allocation base); the borrow
> test held (counter#2 bumped, borrowed, bumped again, closed once). The spike's
> real bug was not R-B1 but the glue's **owned-argument contract**: a hosted fn
> receiving a resource owns that reference and must `resource::release` it (or
> use `resource::with`), else the count never reaches zero and the handle leaks
> silently. **Hard rule for every gate after this.** `spikes/b0-resource/verify.sh`
> is the gate.

### B1 — Substrate: `roc:io/error` + `roc:sync-io/streams`

- `roc:io/error` — `IOErr` as a plain-data nominal (basic-cli's 10 variants),
  the one definition site every package `use`s (D16).
- `roc:sync-io/streams` — `InputStream`/`OutputStream` **resources** (B0) with
  `read!`/`write!`/`blocking` ops. The host crate ships a `Stream` trait; the
  handle wraps `Box<dyn Stream>` so filesystem and sockets provide backings
  (P9's shared substrate). Two backings for the spike: memory (`Cursor`) and
  file.
- **Exit:** a Roc app reads a memory-backed stream and a file-backed stream
  through the *same* `InputStream` resource; stream handles drop-balance.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b1-streams.md)):
> first build — memory backing read 11 bytes, file backing read 720 bytes
> (== `wc -c world.toml`) through one `InputStream`; `live=0`; exit 11. The
> substrate is split into `sync-io-core` (rlib: `Stream` trait + constructors,
> **no `no_mangle`**) that producers cargo-depend on, and `sync-io` (staticlib:
> the hosted symbols) — so bundling the shared rlib into N archives can't
> duplicate exported symbols (H0c). Findings: no `Drop` on bare `RocStr`/
> `RocListWith`, so every hosted list/str arg needs an explicit `.decref` (B0's
> owned-argument rule) — which exposed a **pre-existing quiet leak** in H2/H5's
> stdio components (they never decref their `RocStr` args; tracked cleanup).
> `RocListWith::from_slice` exists for byte lists. `tests/golden/b1-streams/verify.sh`
> is the gate.

### B2 — `roc:cli` + both drivers — first end-to-end milestone

- `roc:cli/{stdin,stdout,stderr}` over `sync-io` streams; `environment`
  (`get-arguments`, `get-environment`, `platform`); `exit`; `terminal-*` (Tty).
- Derived layer: basic-cli's real `Stdout.roc`/`Stderr.roc`/`Stdin.roc`/`Env.roc`
  verbatim modulo the `import Host` rename (R8: migration ≠ composition).
- Drivers: `run! : () => Try({}, [Exit(I32), ..])` (P8) and the
  **`main!`-compat driver** whose adapter calls `get-arguments` and hands the
  app its `main! : List(Str) => …`. This sidesteps the `List(OsStr)` glue gap.
- **Exit:** a basic-cli hello-world using Stdout/Stderr/Stdin/Env **ports
  unchanged** onto the `main!`-compat world and runs; the same logic runs as a
  `run!`-native app.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b2-cli.md)):
> both worlds run with identical output (stdout `hello…`/`user: grayrest`/
> `args: a,b`, stderr `(diagnostic on stderr)`, exit 0). basic-cli 0.21's
> `Stdout`/`Stderr`/`Stdin`/`Tty` are **byte-identical to the cache** (`cmp`
> in `verify.sh`) — zero edits, `import Host` intact — because B2 ships a
> **pure-Roc `Host` module with bodies** reconstructing basic-cli's `Host`
> surface over the WASI-shaped stream primitives (a second derived tier).
> **R-B5 closed by construction:** args/env arrive as `count!`/`at!` primitives
> and the derived `Env.args!`/`dict!` build the `List(Str)` in Roc — no host
> `RocList<RocStr>` at all. `Env` is Str-based per P11 (its four `Path`-typed
> fns move to B3). Compiler findings: `?` won't unify two open error rows
> (use `??`); effectful helpers need `!`; `{} =>` is one unit arg.
> `tests/golden/b2-cli/verify.sh` is the gate.

### B3 — `roc:filesystem` + `roc:path` + `roc:os-path` — the capability model

- Primitive: `Descriptor` resource; `preopens.get-directories!`; `open-at!`
  (bytes path, flags); `read-via-stream!`/`write-via-stream!` (→ `sync-io`
  streams); `stat!`, `read-dir!`, `create-dir!`, `unlink!`, `rename!`,
  `hard-link!`, `set-times/executable` — basic-cli's ~20 file/dir primitives
  re-homed as descriptor ops. **Paths cross as `List(U8)`** (P11).
- `roc:path` (Str; Gleam-style exclude/error on non-UTF-8, WASI-projectable)
  and `roc:os-path` (lossless) — both shipped; the op surface shares a
  `List(U8)` core with two type wrappers.
- Derived: basic-cli's `File.roc`/`Path.roc`/`Dir` ops reconstructed as
  open-at → op → drop over the ambient preopen; **cwd as a userland `cell`
  prefix** (P8), `Env.cwd!`/`set_cwd!` updating it.
- Impls: **unconfined** (a `/` preopen, parity) and **confined** (restricted
  preopen — the seahaven case) as two `roc:filesystem` components.
- **Exit:** basic-cli File/Path/Dir/`Env.cwd` app code runs unchanged over the
  unconfined impl; a D14 world-rename swaps `Path`↔`OsPath` with no app edit;
  the confined impl makes an out-of-preopen path a capability error, not a
  file; `Descriptor`s drop-balance.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b3-filesystem.md)):
> one unchanged app on three worlds — unconfined (`/etc/hosts` readable),
> **confined (`denied`: the capability error — seahaven's confinement as a
> swappable component, P14)**, and `OsPath as Path` (the lossless impl under
> the name `Path` via the new `exports = ["A as B"]` rename-on-copy — D14 made
> real) — all exit 0 (descriptors drop-balanced), host compiled first try.
> Descriptor-relative primitives (P4), bytes paths (P11), NUL-joined listings
> (R-B5), one `stat_at!` for eight leaves, `fs-core` rlib + two thin
> staticlibs via an `exports!` macro (H0c-safe), cwd as a `cell` prefix (P8).
> Finding worth a rule: BSD `sed` has no `\b` — the first OsPath was a silent
> Str copy that *typechecked* because the surfaces match by design; `verify.sh`
> now asserts declaration content, not just types. Deferred: `File.Reader`
> line-buffering, `U128` times, a cached root descriptor.

### B4 — `roc:clocks` + `roc:random` + `roc:locale` + `roc:url`

- `clocks/wall-clock.now!` (→ datetime record), `monotonic-clock.now!`,
  `Sleep` (`sleep-millis`). `random/random.seed-u64!/u32!`. `locale`'s two
  symbols under basic-cli's real 200-line pure `Locale.roc`. `url` as a pure
  package.
- **Exit:** basic-cli `Utc`/`Sleep`/`Random`/`Locale`/`Url` app code unchanged.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b4-small-packages.md)):
> seven checks correct, exit 0. **Six basic-cli modules ship byte-identical**
> (`Utc`, `Sleep`, `Random`, `Locale` 324 lines, `Url` 1,391 lines pure,
> `InternalDateTime`) — the `Host` shim grew six leaves, zero edits; verbatim
> count now 10. `U128` crosses as `u128`. `locale_all!`'s `List(Str)` via
> `count!`/`at!` (R-B5). Three per-package host crates. Findings: binding
> modules take a `*Host` suffix where basic-cli owns the plain name (`Random`);
> `() =>` is zero-arg (`Utc.now!()`); and **`roc check` fails on a warning** —
> a constant-folded `match` is "unconditional" and silently stops `build.sh`
> before `roc build` — fixed in the app (runtime input), discipline kept.

### B5 — `roc:subprocess` (seahaven's design)

- seahaven's `Cmd.roc` (13 fns: exec/output/status/inherit-stdin + PATH
  search) as the derived layer over `cmd-exec-*` primitives; roc-native, no WASI
  precedent (P3).
- **Exit:** seahaven's `Cmd` derived layer runs unchanged; a PATH-searched
  command executes with captured output and status.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b5-subprocess.md)):
> host compiled first try — PATH-searched `echo` with captured output,
> `exit-code: 7`, `check_available!("ls")` running seahaven's full PATH-search;
> exit 0. **seahaven's 499-line `Cmd.roc` ports with exactly two changed
> lines** (the `Env.var!` sites, bridged by one appended `env_var_os!`) and its
> 247-line `OsStr.roc` ships byte-verbatim (`verify.sh` bounds the diff at 2).
> Design call: the crossing record keeps seahaven's `NativeOsStr` union (its
> seam and PATH-split match on `to_raw`'s variants) — P11 governs the path
> packages, not this argument crossing. `Path`/`OsPath` gained seahaven's
> constructors, `is_executable!`, and `is_eq`/`to_hash` (its `expect`s compare
> `List(Path)`). Open: whether `RocList<T,true>::decref` releases elements.

### B6 — `roc:sync-sockets` (full) + `roc:sync-http`

- Sockets: WASI's full surface — `network`, `instance-network`,
  `ip-name-lookup`, `tcp`/`tcp-create-socket` (client **and** listen),
  `udp`/`udp-create-socket` (P6). Sockets are resources; reads/writes go through
  `sync-io` streams. basic-cli's `Tcp.roc` as derived sugar.
- Http: `roc:sync-http/outgoing-handler.send!` over a `roc:http` **types
  package** (Method/Request/Response, mirroring basic-cli's shared-package
  choice, P10). Blocking (P12).
- **Exit:** tcp echo client↔server in-process, a udp round-trip, an http GET;
  basic-cli `Tcp`/`Http` app code unchanged.

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b6-net.md)):
> hosts compiled first try — `tcp-echo: hi`, `http-get: hello-http`,
> `udp-echo: dgram`, `tcp-accept: ping`, exit 0 (`live()==0`). basic-cli's
> `Tcp.roc`/`Http.roc`/`InternalHttp.roc` ship **byte-verbatim** (15 verbatim
> modules total); `Udp.roc` is new sugar. `Host.TcpStream` is an *alias* of
> `Sockets.TcpSocket`; `tcp_input!/output!` mint `roc:sync-io` streams from a
> cloned socket, so sockets and files share one stream layer. `HttpHost.send!`
> takes basic-cli's exact host-request record; response headers cross
> NUL-joined (R-B5) and the shim splits them. Tool gained `[packages]` in
> `world.toml` → `packages { http: "…" }` (verbatim `Http.roc` imports
> `roc-lang/http`). **Finding:** basic-cli's `Method→U8` is `to_host_method`'s
> table (`CONNECT=0 … TRACE=9`, `Unknown` shares `QUERY`'s 2 via `method_ext`),
> not union order — the test httpd now answers only a `GET`, so it's under
> test. Peers are `roc:testnet` host threads (TEST SCAFFOLDING, not shipped).
> The HTTP client is a ~40-line `std::net` one (no TLS/chunked/redirects); B8
> decides whether the shipped host grows a real client behind the same leaf.

### B7 — `roc:temporal` (`temporal_rs`)

- Calendar ops host-backed by `temporal_rs` (P13). Value types (`PlainDate`,
  `PlainTime`, `Duration`) as plain-data records; `ZonedDateTime`/`TimeZone`/
  `Calendar` as **resources**. `temporal_rs` vendored by this one crate only
  (H0c); opt-in per world.
- **Exit:** date arithmetic and a timezone conversion via `temporal_rs`;
  `ZonedDateTime` resources drop-balance; a world without `temporal` links no
  `temporal_rs` (`nm`).

> **Outcome ✅ COMPLETE 2026-09-04** ([note](../notes/2026-09-04-b7-temporal.md)):
> host compiled after one type fix — `2024-01-31 + 1 month = 2024-02-29`
> (constrain), `until = 359 days`, ISO day-of-week, and
> `2024-03-10T06:30Z` rendered as `01:30-05:00[America/New_York]` then
> converted to `15:30+09:00[Asia/Tokyo]` with the epoch preserved; exit 0
> (`live()==0`). `PlainDate`/`PlainTime`/`Duration` are plain records;
> `ZonedDateTime`/`TimeZone`/`Calendar` are resources wrapping `temporal_rs`
> values; calendar-aware ops take an explicit `Calendar`. `temporal_rs =0.2.6`
> (default `sys-local`, bundled tzdb) is vendored by `temporal-host` only —
> `verify.sh` `nm`s every archive and the temporal-less B4 binary (H0c ✅,
> R-B2 resolved). **Findings:** glue emits tag names verbatim as Rust fields
> (`Type` → `pub type:` won't parse; renamed `TypeErr`); glue dedups identical
> result types (`zdt_with_time_zone!` returns `TemporalZdtFromEpochNsResult`);
> `temporal_rs` keeps `ErrorKind` private (host classifies on Debug form);
> the pinned compiler has `U8.to_str`/`I64.to_str`, not `Num.to_str`.

### B8 — `roc:basic-cli` world + the migration proof + the confinement swap

- The all-in-one world: every package above + the `main!`-compat driver +
  unconfined fs, published via `hematite publish` (P14).
- **Migration proof:** a real basic-cli example app builds and runs by changing
  **only** its platform URL.
- **Confinement swap:** the same app world with the confined `roc:filesystem`
  impl wired instead — seahaven-as-a-component.
- **Exit:** the example app runs unchanged on the baseline; the confined
  variant refuses an out-of-preopen path; `hematite tier` reports a
  pure-Roc extension as Tier 1 over the published baseline.

---

## Post-v1 (ordered)

1. **WASI-ABI projection pass** — `roc:filesystem`/`sync-io` signatures move to
   descriptor+stream ABI, D3 reporter reaches distance 0; the `.wit` projection
   (H1) implemented.
2. **Async `roc:http` / `roc:sockets` / `roc:io`** — the clean names (P7),
   once Roc has an async story; uses D8's wake machinery.
3. **Windows `roc:os-path` variant** (P11) when Windows is a target.
4. **Sqlite world** (P3) — its own composition over the baseline.

---

## Risks

- **R-B1 — the `roc_dealloc` pointer contract** (B0). ✅ RESOLVED 2026-09-04:
  Roc passes the allocation base; registry keyed on `data − 8` hits. The live
  risk it uncovered instead is the **owned-argument rule** — a hosted fn that
  takes a resource must `resource::release`/`with` it or the handle leaks
  silently (no crash). Every B1–B7 hosted signature taking a resource is a
  place to get this wrong; `verify.sh` gauges must assert `live=0`.
- **R-B2 — `temporal_rs` weight and vendoring.** Heavy; must be the sole
  vendor of its natives (H0c) and opt-in per world.
- **R-B3 — full sockets surface is large.** tcp listen + udp + lookup is
  well beyond basic-cli; B6 is the widest gate.
- **R-B4 — two path packages drifting** on their op surface. Mitigated by a
  shared `List(U8)` core; B3's exit diffs the two wrappers' surfaces.
- **R-B5 — the `main!`-compat adapter's `List(Str)` construction.** Building
  the args list host-side needs a `RocList<RocStr>` constructor the pinned glue
  lacks (H2). B2 either builds it from primitives or, if unsound, the compat
  driver passes args via a `get-arguments!` call the derived `main` wrapper
  makes on the Roc side (no host marshalling at all).

## Non-goals (v1)

- Real WASI-ABI / `wasi:*` namespace (P2) — post-v1 projection pass.
- Async I/O of any kind (P12) — `sync-*` only.
- `wasi:io/poll`, `clocks/timezone`, `random/insecure*` (P6).
- Sqlite (P3) — separate world.
- Explicit `own`/`borrow` (P5) — refcounting subsumes them.
- Windows path variant (P11).

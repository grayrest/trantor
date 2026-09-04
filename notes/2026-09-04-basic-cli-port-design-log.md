# basic-cli → hematite port — design log (P1–P14)

Decisions from a design session on porting **basic-cli** onto hematite by
splitting it into interfaces (grouped à la WASI) and implementing each in a
crate. **Pre-implementation** — no code yet; this records the resolved design
tree. Numbered in resolution order; each depends on those above it.

Companion to the hematite design log ([`2026-09-04-hematite-design-log.md`](2026-09-04-hematite-design-log.md),
D1–D23); references to `Dn` are its decisions.

## Evidence the decisions rest on

Authoritative basic-cli surface read from the cached 0.21 package
(`~/.cache/roc/packages/4rAQg8kU…`): **20 app modules**, **~55 hosted symbols**.
seahaven is basic-cli trimmed (its README) and is the proxy for the confined
case. Key facts that drove decisions:

- basic-cli models resource handles as `FileReader :: Box(U64)` / `TcpStream ::
  Box(U64)` — opaque host handles threaded back on each call; no `close` symbol
  (cleanup is refcount decref).
- basic-cli is **path-based with ambient authority** (`File.read_utf8 : Path`,
  `Env.cwd!`/`set_cwd!`); seahaven bolted cap-std on to confine it.
- basic-cli is **fully synchronous** (blocking host calls: `http_send_request`,
  `tcp_read`, `file_read`).
- basic-cli's entrypoint passes **args as a parameter**
  (`main! : List(OsStr) => Try({}, [Exit])`) and pulls HTTP data types from a
  **shared `roc-lang/http` package**, platform supplying only `send!`.
- The pinned glue already ships `decref_box_with(… payload_decref …)` and its
  doc names the `Box(U64)` host-resource case; the glue-redesign runtime matrix
  already tests decref destructors / balanced allocations.
- Roc's `Str` is **strict UTF-8** (validated), which places Roc with Gleam/Swift,
  not with Go's loose byte-strings — decisive for the path type (P11).

## Decisions

**P1 — WASI groupings are a projection target, not just a taxonomy.** Group
WASI-shaped now so a future WASI-ABI adoption is a within-interface swap, not a
re-grouping. This first pass keeps basic-cli's ergonomic signatures; D3's
projectable-subset reporter measures each interface's distance to real WASI.
Rationale: WASI is a committee's multi-year answer to "carve up host
capabilities for an isolated, no-stdlib runtime" — exactly Roc's situation — so
it is a considered *starting point*, not mere nomenclature.

**P2 — Namespace is `roc:*`, mirroring WASI's package/interface split
one-to-one; `wasi:*` is reserved for real-ABI.** `roc:filesystem/types`,
`roc:cli/stdout`, `roc:clocks/wall-clock`. Naming an interface `wasi:` while it
carries basic-cli signatures would claim a contract it does not honor (D3: report,
don't lie). When the ABI actually becomes WASI's, `roc:filesystem` becomes
projectable to `wasi:filesystem` (distance 0). Corrects the fixtures'
`roc:cli/filesystem` — filesystem is its own WASI package, not under cli.

**P3 — The three non-WASI surfaces.** Sqlite → **its own world**, out of scope
for this port. Subprocess → **roc-native `roc:subprocess`, using seahaven's
design** (13 fns incl. PATH-search), not basic-cli's thinner 5; WASI has no
subprocess interface. Locale → **roc-native**, and it is ~200 lines of *pure*
BCP-47 parsing over just 2 host symbols (`locale_get!`/`locale_all!`), so it is
a pure package plus a tiny interface.

**P4 — Filesystem: descriptor/preopen capability primitives, basic-cli path API
as derived sugar.** The primitive interface is WASI's capability model
(`descriptor` resource, `preopens`, `open-at`, read-via-stream); `File.read_utf8
: Path => …` is reconstructed in the derived layer as open-at → read → close.
Rationale: the descriptor model **is** the isolation that makes WASI fit Roc
(the reason cited in P1); it makes seahaven's confinement fall out of the
primitive layer for free (a confined impl hands out restricted preopens) instead
of being a fork. **This drives a hematite feature: real WIT `resource` types**
(D3 anticipated them; the fixtures never built them).

**P5 — A `resource` is a refcounted opaque host handle; refcounting *is*
ownership.** `Box(<opaque>)` carrying a raw host pointer + a host `payload_decref`
destructor; the destructor fires when the last Roc reference drops. The IDL gains
a `resource` keyword. WASI's manual `own`/`borrow` and `i32` handle-tables exist
only because WASM lacks refcounting; Roc *has* refcounting, so `own`/`borrow` are
**subsumed for free** (pass-by-value = borrow, last drop = own) and memory-safe
by construction. The decref-destructor mechanism is already present in glue
(`decref_box_with`) and tested upstream, so hematite's work is to *confirm*
drop-balance in a composition (a host open/close gauge), not discover it. Lowers
to WASI's `i32` handle on the wasm edge (D22) — a lowering concern only.

**P6 — Interface granularity: WASI's boundaries, populated where basic-cli
reaches — except sockets, which is taken in full.** Every basic-cli function
goes into the `roc:` interface WASI would put it in (so future ABI adoption never
moves a function between interfaces), but no empty shells for capabilities the
platform lacks. **Sockets is the deliberate exception**: the full WASI surface
(tcp client+listen, **udp**, network, ip-name-lookup, create-socket), not
basic-cli's client-only Tcp — "more work upfront, best long-term." Skipped:
`wasi:io/poll` (async), `wasi:clocks/timezone`, `wasi:random/insecure*`.

**P7 — `sync-*` namespace for blocking I/O whose async version is the better
future.** `roc:sync-io` (streams), `roc:sync-sockets`, `roc:sync-http` carry the
prefix — reserving the clean names for the async versions Roc will eventually
want (WASI 0.3 is going async-native, so blocking is the *inferior* one).
`roc:filesystem`, `roc:cli`, `roc:clocks`, `roc:random` stay clean (no better
async version, or not I/O). Accepted consequence: filesystem/stdio functions
return `roc:sync-io` streams today; when async streams land, those return types
migrate `sync-io → io` — a localized within-interface change, which is exactly
what P1's staging exists to absorb.

**P8 — Entrypoint is WASI-shaped `run! : () => Try({}, [Exit(I32), ..])`;
capabilities arrive via interface calls.** No entrypoint parameters; the app
calls `roc:cli/environment` (args, env), `roc:filesystem/preopens`,
`roc:cli/stdin`/`stdout`/`stderr`. basic-cli's `main! : List(Arg) => …` survives
as a **derived compat driver** (adapter calls `get-arguments` and hands the app
its `main`). **cwd becomes a userland `cell` prefix over preopens** — a
capability model has no ambient cwd, so `Env.cwd!`/`set_cwd!` become derived
convenience within granted preopens (exactly how WASI libc shims fake cwd).
Bonus: this sidesteps the `List(OsStr)`-marshalling glue gap (H2) — args come
from a call, not the entrypoint. One non-transparent migration: an app that
`set_cwd!`'d to escape its directory now gets a capability error — which is the
isolation being bought.

**P9 — One host crate per WASI *package*, plus shared substrate crates.** A
component exports the package's interfaces: `roc:cli`, `roc:filesystem`,
`roc:sync-sockets`, `roc:sync-http`, `roc:clocks`, `roc:random`,
`roc:subprocess`, `roc:locale`. Plus **`roc:sync-io`** (the `Stream` trait +
stream-resource machinery, a shared cargo library) and **`roc:io/error`** (the
`IOErr` value type). Refines the original "each interface in a crate": per-package
groups what shares host state (the socket table; the stream machinery), and the
real substitution boundary is the capability (swap the whole filesystem impl),
not the sub-interface. `roc:sync-io` as a shared cargo dep is legitimate (shared
*library* code like `abi`, not a cross-component hosted call — the H2 rule).

**P10 — Cross-cutting types split by "does it need the host?"**
Interface-owned (bodiless, cross the boundary): `IOErr` → `roc:io/error` (a
*value*, not a resource); `input`/`output-stream` → `roc:sync-io`; `datetime` →
`roc:clocks`. Pure packages (pure data + ops, used across interfaces):
`Path`/`OsPath` → `roc:path`/`roc:os-path`; `Url` → `roc:url`; HTTP
`Method`/`Request`/`Response` → a `roc:http` types package (mirroring basic-cli's
own shared-package choice; the effectful `send!` is the interface). Keeps D7 (no
blessed vocabulary) and D16 (one nominal definition site, `use`d never
redeclared) intact.

**P11 — Paths: bytes primitive + two derived path packages, both shipped,
swapped by world-rename.** Resolved after research: Gleam (Roc's closest peer —
strict-UTF-8 `String`), WASI, and seahaven all chose UTF-8 with non-UTF-8
*excluded/errored*; Rust/Swift chose a separate lossless type; Go sidesteps it
with loose byte-strings. Roc's strict `Str` places it with Gleam. **Resolution:**
the primitive filesystem interface takes **raw path bytes (`List(U8)`)**, and the
path *type* is a derived-layer choice — `roc:path` (Str-based, default,
WASI-projectable, Gleam-style exclude/error on non-UTF-8) and `roc:os-path`
(lossless, beyond-WASI, D3 reports non-projectable). **Both ship in the baseline
always**, so choosing is a pure **D14 world-rename** (expose one as `Path`), with
app-level `import … as Path` as an escape hatch. Zero host duplication — both
derived over one bytes primitive. The Windows-`U16` OsStr variant is deferred
until Windows is a target (a within-`roc:os-path` change). This dissolves the
OsStr-vs-UTF-8 debate by refusing to make it a platform-wide commitment.

**P12 — Sync I/O is plain blocking hosted calls; no D8 machinery.** Every
`sync-*` call blocks the runtime thread — no executor, no `Sink`, no `wake`.
**The entire basic-cli port needs zero async infrastructure**, and the `run!`
driver is the trivial driver (call `run!`, get `Result`, exit) already proven in
H2/H5. This is the sharp division of labor with the roc-solid port: roc-solid is
the async/executor (D8) stress test, basic-cli is the all-synchronous one. Only
foreclosed capability — concurrent in-flight I/O — is what the future async
`roc:http` (clean name) would add; basic-cli can't do it either.

**P13 — `roc:clocks` WASI-minimal + host-backed `roc:temporal`.** `roc:clocks` is
`wall-clock.now`/`monotonic-clock.now` only (WASI's clocks has no calendar math).
Calendar operations are **host-backed by `temporal_rs`** (the vetted Rust
Temporal implementation), not reimplemented in Roc — timezone DBs and calendar
systems are exactly what not to port by hand. Representation is **cost-based**:
value types (`PlainDate`, `Duration`) as plain-data records (inspectable,
host reconstructs per op); context-bearing types (`ZonedDateTime`, `TimeZone`,
`Calendar`) as **resources** (opaque `temporal_rs` handles, avoid re-parsing) —
the second real exercise of P5's resource model. `temporal_rs` is vendored by the
one `roc:temporal` crate (H0c: no duplicate natives) and, being heavy, is opt-in
per world (a date-free CLI doesn't link it — the colorhunt-sans-sqlite property).

**P14 — Baseline: modular packages underneath, an all-in-one `roc:basic-cli`
world as the migration front door.** *(Confirmed.)* The
modular interface packages are the real artifact (each independently composable,
so a lean app links only what it wires). `roc:basic-cli` is a published all-in-one
*world* built from them — every standard interface + native impls + the
`main!`-compat driver — consumed by URL so an existing basic-cli app ports by
changing the platform URL. Excludes sqlite (own world), includes http/tcp/temporal
for 0.21 parity. **Confinement/preopens is a swappable filesystem impl**: the
baseline hands out a `/` preopen (ambient parity); seahaven's confinement is a
`roc:filesystem` impl that hands out a restricted preopen — the same interface,
so seahaven stops being a fork and becomes a component. This is the hematite
thesis landing on the platform it was built for.

## Open items (next session)

1. ~~P14 baseline shape~~ — confirmed all-in-one-parity (2026-09-04).
2. **Sequencing / execution plan** — resolved in
   [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md).
3. **Temporal value-vs-resource boundary** (P13) — the exact cutoff between
   record-represented and resource-represented Temporal types is a recommendation,
   not a settled list.
4. **`own`/`borrow`** deliberately deferred (P5 — refcounting subsumes them);
   revisit only if a use-after-move bug surfaces that refcounting doesn't catch.
5. **Windows `os-path` variant** deferred until Windows is a target (P11).
6. **hematite features this port drives**, to fold into hematite's own plan: the
   `resource` keyword + refcounted-handle codegen (P5), the bytes-primitive path
   convention (P11), and confirmation that D14 world-rename covers the path swap
   (P11). The resource work is the load-bearing prerequisite and should be a
   hematite gate before the port starts.

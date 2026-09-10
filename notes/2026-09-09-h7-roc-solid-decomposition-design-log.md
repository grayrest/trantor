# H7 — roc-solid decomposition: design log (grill 2026-09-09)

**Plan:** [`plans/2026-09-09-h7-roc-solid-decomposition.md`](../plans/2026-09-09-h7-roc-solid-decomposition.md).
Closes the PARTIAL left by [`2026-09-04-h7-platform-im-slice.md`](2026-09-04-h7-platform-im-slice.md).

Decisions are numbered in resolution order; each depends on those above it.
Everything here was settled against the tree as of roc-solid `7f301ffc`
(2026-09-03) and hematite `142c981`.

## What the tree said before any question was asked

- **`HostOp(Str, Str)` did not die; it was renamed.** roc-solid's escape hatch
  is now `Cmd.Service(request, route_key, name, payload)` answered by
  `Event.Service({ seq, last, ok, body })`, with space-split text protocols
  (`notes`: `list` / `read <name>`; `doc`: eleven verbs; `spawn`: a binary and
  its args). roc-solid's own rule splits the union: platform-wide commands stay
  typed (`Fetch`, `Http`, `ImageLoad`, `CopyRow`); anything "one app and one
  host agree on" goes through `Service`. The plan's north star — the stringly
  hatch replaced by typed components — is unchanged, only the name moved.
- **The union host grew Cargo features instead of components** (2026-09-01:
  `sqlite`, `net`, `audio`, `pdf`, `clipboard`, `eink*`). Today's `libhost.a`
  has 0 sqlite symbols, and today's `colorhunt` binary has **280**, because
  `just im-host` defaults to `--features net,sqlite`. So H7's exit is
  flag-satisfiable and mechanism-untested — which is what the plan said would
  not count ("a migration that leaves HostOp in place has not tested anything").
- **The engine reaches every service one of two ways.** Sync (`notes`, `doc`,
  `spawn` return `abi::Event` values; the engine routes them) or worker-thread
  (`dbx` via `worker.rs`, `net` via `tasks.rs`, images via `imgwork.rs`) — plain
  Rust data comes back through a `wake: Fn()` closure over the winit
  `EventLoopProxy`, and the Roc `Event` is built on the runtime thread because
  nothing Roc-side is `Send`. That is D8's `HostCtx.wake` + D9's opaque
  completion, already written in roc-solid's own words.
- **Audio's state is ambient, not an event:** `Env.playhead` / `Env.mic_level`
  change every frame and the app only reads them. A service can therefore own
  a piece of the per-frame `Env` record, not only union variants.
- **191 Roc apps and ~150 gates bind to `platform-im/main.roc`;** 152 Justfile
  recipes depend on `im-host`. Gates for dbx / net / notes live inside host-im
  (`gdbx.rs`, `gnet.rs`, `gtext.rs`, `gnotes.rs`, ~1.8k lines) and call service
  internals directly.
- **The signals `platform/` is dormant in git (2026-07-19) but live in the
  Justfile:** `glue`, `counter*`, `todo*` and `just check` still build it.
- **`crates/host-dom` is a second driver on the same glue** — 36 `CmdTag` /
  `EventTag` arms, its own runtime symbols, `Cmd.Service` lowered to a POST to
  `/@service/<name>` answered by `serve.py`. Its recipes are not in `im-check`.
- **roc's linker passes every `inputs` entry for every target**
  (`src/cli/linker.zig:863`), so multiple wasm32 inputs are plausible, not
  proven.
- **glue's nominal hazard is positional:** a *named* nominal wrapping a nominal
  in an exposed proc's **return** miscompiles ("264 bytes out of bounds");
  "nominals are safe as fields and as payloads". A wrapper variant carrying a
  component's union rides the safe side — to be measured, not assumed (P0).

## Decisions

**D-H7-1 — Target: finish H7 (platform-im), not the signals `platform`.** The
signals platform is the post-v1 #2 item, ordered after tower-platform, and
untouched since July; platform-im is where roc-solid is active (eink, doc,
scene — all September). H7 is the plan's PARTIAL gate and its exit is the
mechanically checkable one.

**D-H7-2 — Typed composed unions, not a Service registry.** A registry (keep
`Cmd.Service`, compose the `name → handler` table) would test only the link
set. The plan's exit criterion is typed effects; `Service` is the hatch by a
new name, and the union-host disease is precisely that a stringly channel let
app-specific host code accrete inside the shared platform with no type to
object. Rejected: "registry first, typed second" — two migrations of 191 apps.

**D-H7-3 — Scope: every service.** `notes`, `spawn`, `dbx`, `net`, `audio`,
`doc` become components; `Cmd.Service` / `Event.Service` are deleted at the
end. Stays in the driver, by kind not by size: `eink/` (a `Surface`, the other
half of the render loop, not a service), `assets`/`imgref`/`imgwork`
(`ImageLoad` is rendering: decoded pixels feed the atlas), `maps`
(`ReadRoads` reads the map's own declared archive), `shell` (location, title,
storage — host-owned state the app must not be able to disagree with),
`hostres` (the resource table the renderer reads — a service *borrows into*
it, see D-H7-11), clipboard (`Copy` is OS integration, already its own
feature). Rule of thumb that fell out: a thing is a service if an app can be
written without it *and* it owns a dependency the platform should not carry.

**D-H7-4 — Layout: `platform/<world>/` in roc-solid; components by `path =`.**
`platform/clay/` (the native driver, every service — the baseline the 191
gate apps bind to), `platform/dom/` (the DOM driver), per-app
`platform/colorhunt/`, `platform/dbx/`, `platform/conduit/`,
`platform/notesviewer/`, and the signals platform moved unchanged to
`platform/signals/`. `platform-im/` is retired: its 55 modules move to
`crates/host-im/roc/` because they are the *driver's contract*, shipped by the
driver component. hematite gains `[components.x] path = "../../crates/…"`
(default stays `components/<name>/`) so a crate has one home. Rejected:
symlinked `components/`; hematite fixtures vendoring roc-solid (two copies,
no real app migrates); per-app worlds under `apps/<app>/` (worlds in two
places).

**D-H7-5 — Per-component union + generated wrapper.** A service ships one Roc
interface module — `Notes.roc` with `Cmd := [List(U64, Str), Read(U64, Str,
Str)]` and `Event := [Listing(…), Text(…)]` (and `Env := {…}` for audio).
hematite generates the world's `Cmd := [<core…>, Notes(Notes.Cmd), Dbx(Dbx.Cmd)]`,
`Event` likewise, and the app writes `Cmd.Notes(Notes.Cmd.List(0, "k"))`.
Why this over a flat merged union: glue emits each component's union as its
own Rust type, so a component's `extern "C-unwind"` handler signature is
**world-independent** — it compiles once against `Notes.Cmd`, and the
driver's generated dispatch is one arm per component. A flat union would give
every component a different `CmdTag` per world and force per-world codegen
inside the component. One module per service also fits the existing manifest
(one `module` per interface) with no schema change beyond naming which types
splice. Rejected: `interface.toml` type-text entries (types in TOML strings
nothing checks); Roc fragment files (no schema at all).

**D-H7-6 — Splice markers in driver-owned modules.** `Cmd.roc`, `Event.roc`
and `Env.roc` stay authored by the driver (their ~300 lines of boundary
documentation and the core variants stay put) and each carries one marked
block — `## @hematite(cmd)` / `(event)` / `(env)` — that hematite fills with
the components' wrappers. One mechanism covers the two unions *and* the `Env`
record (which has methods and could not be generated whole). This extends
D18-C ("the driver's contract text is spliced") and does not breach D13: the
marker is the driver *declaring* a splice point in its own contract, not
hematite rewriting a component's source.

**D-H7-7 — Contract: sync return + `HostCtx` wake (D8/D9 built).** Per
component, generated into the world's abi crate and called by the driver:
`hematite__<c>__init(*const HostCtx)`;
`hematite__<c>__cmd(request, route_key, <C>::Cmd) -> RocList<(route_key, <C>::Event)>`
for sync answers; async work calls `ctx.wake(component_id, token)` from any
thread and the driver, on its runtime thread, calls
`hematite__<c>__complete(token) -> Completion { request, route_key, event }`;
`hematite__<c>__env() -> <C>::Env` once per frame for a component with an env
block; `hematite__<c>__gate(name) -> i32` (D-H7-8). The component never names
the driver. Rejected: the component calling a driver-exported route symbol —
bakes the driver's name into every component and the archive-order direction
of that reference is unmeasured here.

**D-H7-8 — Gates move with the service they test; the driver asks components
first.** `gdbx.rs` → `svc-dbx`, `gnet.rs`/`gtext.rs` → `svc-net`, `gnotes.rs`
→ `svc-notes`. The driver's 149-arm argv dispatch tries
`hematite__<c>__gate(name)` on every component (−1 = not mine) before its own
arms, so `im-check`'s gate list and every recipe's invocation are unchanged.
Rejected: cargo-tests only (loses the end-to-end Roc-app gates); the driver
depending on service crates in the baseline (re-creates the union host inside
`platform/clay`).

**D-H7-9 — The DOM host stays compiling: `platform/dom` is in scope, and each
component's `.wasm` is listed in `inputs`.** host-dom becomes the second
driver world with its own components (`svc-notes-dom`, `svc-net-dom` — the
same interfaces, JS transport: the H5 substitution thesis on a second
target). `hematite build --target wasm32` generalises `just dom-host` per
component (nightly `build-std`, `-Cpanic=immediate-abort`, `llvm-ar x`,
`wasm-ld -r` per archive) and lists every relocatable in
`wasm32: { inputs: [...] }` rather than pre-merging into one `host.wasm`.
**Unmeasured**, so P0 measures it first; if roc's wasm32 link rejects
multiple inputs, that is raised as a decision (merge with `wasm-ld -r
--whole-archive` is the fallback), never switched to silently. The H0c scan
reads wasm archives through `llvm-nm`.

**D-H7-10 — The abi crate is `hematite_abi` everywhere.** roc-solid's ~8
import sites (`host-im` ×5, `host-dom`, `ir`) become `use hematite_abi as
abi`; `crates/abi` is deleted (glue output is per-world now) and its
hand-written `http.rs` moves into `svc-net`, whose `net.rs` is its only
reader. Rejected: a `[world] abi_crate` knob — two names for one thing.

**D-H7-11 — Rust-heap boundary rule.** Every staticlib carries its own
allocator shims and host-im's `#[global_allocator]` (mimalloc) governs *its*
archive only, so a `Vec`/`Arc` allocated in one archive and dropped in another
is a cross-allocator free. Nothing Rust-allocated crosses an archive boundary:
crossings are Roc values (`roc_alloc`, single provider) or C-ABI tokens owned
by the side that allocated them (D9). Consequence for `doc`: a page group's
`Scene` stays owned by `svc-doc`; the driver *borrows* it by pointer between
`group` and `drop`/`close` through `HostCtx` (`register_group(handle, *const
Scene, generation)` / `release`), and never frees. P0 measures the symbol
class of `___rust_alloc` per archive so the scan's exemption model records it.

**D-H7-12 — Baseline first, extraction second.** `platform/clay` is composed
with **zero** services extracted before the first one moves: host-im as an
`authored_host` driver via `path`, its modules shipped, the 191 apps re-pointed,
`im-check` green. That checkpoint is behaviour-identical by construction and
turns every later phase into "one service moved, same gates". Extraction order:
`notes` (sync, smallest) → `spawn` (streaming) → `dbx` (async + the sqlite
exit; `Fetch`/`Rows`/`CopyRow` leave the core) → `net` (async + canned source)
→ `audio` (the env block) → `doc` (the resource borrow, riskiest) → DOM → the
per-app worlds and the `nm` exit.

## Where `Fetch` belongs

`Cmd.Fetch` is documented generically ("the host performs it") but the engine
builds a SQL `Job` from it and nothing else ever has; the three gate apps that
use it as "the generic async completion" (`im-g4/async`, `im-g3/todo`,
`im-g6/nested`) run in `platform/clay`, which has dbx. So `Fetch` → `Dbx.Cmd.Fetch`
and those apps rewrite one line each. A generic "async completion" the platform
performs is what the typed service components *are*; no core variant remains
for it.

## P0 findings (measured 2026-09-09)

Fixture `tests/golden/im-services/` (imview-slice + `svc-echo` sync +
`svc-tick` async + a `TickEnv` block) and spike `spikes/h7-wasm-inputs/`.

- **R-H7-1 answered: the wrapper union crosses both ways** — `List(Cmd)` out
  of `cmds_for_host`, `Event` into `route_for_host`, and `Env.tick : TickEnv`
  in — and glue emits each service union as its own named Rust type (`Echo`,
  `Tick`, `EchoEvent`, `TickEnv`) with world-independent names. **Provided the
  union has two or more variants.** A single-variant union is unwrapped to its
  payload, and a multi-field single-variant payload is mis-typed as `u64`
  while glue's own `size_of` assert still knows the true size (64 ≠ 32) — a
  loud build failure, not a silent miscompile. P1 rejects a service union with
  one multi-field variant at compose time (`Net := [Http(U64, Str, Str)]` is
  the real case: give it a second variant or a record payload).
- **One nominal per module.** A type module exposes only the nominal named
  after the file; a second `Event :=` in `Echo.roc` is "type not exposed". So
  a service ships `Notes.roc` (its command union), `NotesEvent.roc`, and for
  audio `AudioEnv.roc`; the wrapper is `Notes(Notes)` / `Notes(NotesEvent)` /
  `audio : AudioEnv`, and the app writes `Cmd.Notes(Notes.List(0, "k"))`.
  Amends D-H7-5's "one module with `Cmd`/`Event` inside" to the shape the
  approved option actually named. The interface manifest therefore names up
  to three modules (`module`, `event_module`, `env_module`).
- **A nominal record pattern must name every field** (`Env.{ width: w, tick: _ }`);
  the splice must not break the driver's existing destructures — P1 rewrites
  none, so the driver's `Env` methods that destructure must be written with
  `..`-free full patterns before the block is spliced (host-im's use
  `Env.{ … } = env` today — audit in P7).
- **D8/D9 hold as written:** `HostCtx.wake` from a worker thread, three
  completions built on the runtime thread inside the component's `complete`,
  tokens allocated and freed by the component. The gate chain and the env
  block worked first time.
- **D-H7-11 corrected by measurement.** The Rust allocator shims are NOT
  per-archive: every archive defines `__rust_alloc`/`__rust_dealloc`/
  `__rust_realloc` as **v0-mangled plain-external** symbols with the same
  name (`__RNvCs9wFQrvczXsK_7___rustc12___rust_alloc`), so the roc link
  first-wins them exactly like H0c's vendored natives — one shim serves the
  whole binary, from whichever archive is scanned first. Two consequences:
  (1) the H0c scan's "Rust-mangled ⇒ ODR-identical" exemption is **false for
  these three symbols** when any archive sets `#[global_allocator]` (host-im's
  mimalloc): the winner decides the allocator for every archive, silently;
  (2) with the driver LAST in `archive_order`, a component's default shim wins
  and the driver's mimalloc is dropped — inferred from H0c's first-in-inputs
  measurement (80 vs 4), not re-measured with a `#[global_allocator]` in the
  driver; P2 measures it on host-im (`nm` for `_mi_malloc` reachability from
  `__rust_alloc`) before relying on it. The rule for the plan: the driver's
  archive must be scanned FIRST for the shims, or the scan must reject a
  `#[global_allocator]` outside the driver — see the D-H7-13 question below.
  The cross-allocator-free hazard D-H7-11 feared does not exist (one shim);
  the borrow protocol for `hostres` stays for ownership clarity, not for
  allocator safety.
- **R-H7-2 answered: multiple wasm32 inputs do not link on this compiler**,
  in either form. Two `wasm-ld -r` relocatables: every std / compiler-builtins
  member is a strong definition in both (`__negdf2`…). Two wasm-member
  archives: the same, because roc links its inputs `--whole-archive`, so
  archive laziness never applies. **The merge works:** one `host.wasm` from
  `wasm-ld -r --whole-archive <driver>.a --no-whole-archive <component
  contract members…> <component>.a` — the driver whole, each component rooted
  by the members that define its contract symbols (found with `llvm-nm`; `-r`
  refuses `--undefined`), the rest lazy so std stays single-copy. app → b → a
  resolve; the module runs (`seed=21`, `n=42`, one `roc_alloc`). The pinned
  compiler also requires `exports: […]` on a wasm32 target.

**D-H7-13 — The driver's allocator is the binary's: driver FIRST in
`archive_order`, and the scan enforces it.** (Decided 2026-09-09 after P0.)
`resolve` emits the driver archive first so its shims win the first-wins
link; hosted symbols resolve lazily in both directions (H0e/H0c, and P0's
driver→component references into earlier archives), so the topological order
was never load-bearing for them. The scan stops exempting the three
`___rustc*` shim symbols under the Rust-mangled rule and refuses a
`#[global_allocator]` in any non-driver component — the mechanism plus the
check that keeps it true. Rejected: driver-first alone (a component that later
sets its own allocator loses silently); scan-only (the driver's allocator
still loses to whichever component is scanned first).

**D-H7-9 (revised) — wasm32 staging is ONE merged `host.wasm`.** (Decided
2026-09-09 after P0.) The measured negative overturns "list each component's
.wasm in inputs": roc links wasm inputs `--whole-archive`, so per-component
inputs collide on std / compiler-builtins in either form. hematite merges with
`wasm-ld -r --whole-archive <driver>.a --no-whole-archive <contract members…>
<component>.a`, rooting each component by the members that define the symbols
hematite itself mangled — deterministic, and std stays single-copy. Taking the
laziness upstream was rejected as a blocker on `platform/dom`.

## P1 notes (built 2026-09-09)

Small data-model calls made while building the tool, each within the
approved approach and each flagged here rather than silently:

- **A service interface is marked explicitly: `kind = "service"`** in
  `interface.toml`, beside `module` (the command union), `event_module` and
  `env_module`. Rejected: inferring "service" from the presence of
  `event_module` — audio has commands and an env block but no events.
- **A component's Roc modules are looked up in `<dir>/roc/` first, then
  `<dir>/`** (`manifest::module_path`). A driver crate shipping 55 contract
  modules keeps them out of its crate root; the fixtures' flat layout still
  works. Same rule for every component kind.
- **The contract is uniform: a service with events exports `complete` even
  when it only answers synchronously** (`svc-echo` stubs it `unreachable!`).
  The shim is generated from the manifest, which does not know which services
  are asynchronous, and a link error is the right failure for a missing
  export. A service with no `event_module` has a `cmd` returning nothing and
  no `complete`.
- **`features`/`default_features` are refused with `path`.** The HC0 knob
  rewrites the crate's own `Cargo.toml` in place, and a crate at its own path
  is shared between worlds. roc-solid's feature-flagged host loses those flags
  as its services move out (P3–P8), so nothing needs it; a `--features`
  CLI form is the fix if something does.
- **wasm rooting uses the `hematite__<c>__` prefix, not an enumerated
  contract list.** Every symbol a component owns — hosted leaves and the
  service contract — carries it, so "members defining any `hematite__<c>__*`"
  is complete by construction and needs no second list to keep in step.
- **The wasm scan reads `llvm-readobj` flags**, not `llvm-nm` letters —
  see the nm-scan note's 2026-09-09 amendment; and `rust_eh_personality`
  joins the allocator shims as a first-wins Rust runtime singleton.

## P2 decisions (2026-09-09)

**D-H7-14 — `path` components build inside their HOST workspace, with the
world's abi patched in per build.** A crate belongs to exactly one cargo
workspace; roc-solid's root already owns `crates/host-im`, and several
worlds (clay, dom, colorhunt…) name the same crates, so hematite's per-world
workspace cannot list them ("member of the wrong workspace"). With `[world]
cargo_root = "../.."` hematite emits no workspace and runs `cargo --config
'patch.crates-io.hematite-abi.path="<world>/abi"' build --release -p
<package>` per component in the host workspace, staging from its `target/`.
Components declare `hematite-abi = "0.0.0"` (a crates-io name that does not
exist); the host's root `Cargo.toml` carries a default `[patch.crates-io]` to
its baseline world's abi so its own `cargo clippy/test --workspace` keep
resolving, and one target dir is shared by every world. Measured in a probe
and by `tests/golden/cargo-root`: the override takes effect per build and
`Cargo.lock` does not churn between worlds. Rejected: excluding the crates
from the host workspace (loses its clippy/test coverage, one target dir per
world, and two worlds still cannot share a crate); symlinks under
`components/` (the target still sits under the host workspace — the same
conflict).

**D-H7-15 — roc-solid finds hematite at `~/.bin/hematite`**, overridable by
`HEMATITE=…` — the pinned-tool discipline roc-solid already applies to `roc`
and `RustGlue.roc`. Rejected: the sibling checkout's `target/release` (couples
the build to another working tree's state, the hazard the pinned-roc note
records).

## P2 findings (2026-09-09)

- **The migration is behaviour-identical.** With every service still inside
  host-im, `just im-check` against the composed `platform/clay` passed
  133/134 gates on roc-solid's previous compiler (the one miss was a probe
  script under `notes/` still spelling `platform-im/`). `im-host` = `hematite
  build platform/clay --platform-only`: compose, glue, cargo inside the root
  workspace with the clay abi patched in, stage `libhost_im.a`, sysroot, scan.
- **D-H7-16 — one compiler: `~/.bin/roc`, whatever a repo's comment says it
  pins.** roc-solid's Justfile called `~/.bin/roc` its pinned copy (a7d4d3);
  `~/.bin/roc` had since become b07d7e (hematite's plan pin), on which
  roc-solid no longer compiled: `List.sort_with`'s comparator is `[Before,
  After, Same]` now, not `[EQ, GT, LT]` (Grid.roc's ordering helpers, three
  colorhunt files, `im-map/Route.roc`). Decided: everything builds with
  `~/.bin/roc`; roc-solid is ported as part of P2 rather than kept on a
  stale pin. Rejected: per-repo stamps (two compilers to reason about for one
  boundary); restoring `~/.bin/roc` to a7d4d3 (re-measures nothing, keeps the
  hazard).
- **A framework sysroot needs `Versions/` too.** `.tbd` stubs re-export
  siblings by install name (`…/Versions/A/CoreImage`), resolved under the
  sysroot; hematite's generator linked only the `.tbd` and 23 frameworks
  failed to link. turso's lone CoreFoundation never re-exported anything, so
  the fixture could not have caught it. Fixed in `build.rs` (plus
  `PrivateFrameworks`, as roc-solid's recipe had).
- **Interim placements, to be undone by later phases:** `abi/http.rs` is
  parked in `crates/ir` (both hosts read it; P6 moves it into `svc-net`);
  host-dom's 17 wasm exports are declared on host-im's `driver.toml` so the
  clay world keeps emitting the wasm32 target the DOM host links against (P9
  splits `platform/dom`). `just dom-app` was already failing on `~/.bin/roc`
  before P2 — the compiler now requires `exports:` — so the clay world's
  target line is a strict improvement.
- **cargo touches the crates.io index once** for `hematite-abi = "0.0.0"`
  when the lock first records the patched path; later builds do not.

## P3 decisions (2026-09-09) — the first service out

**D-H7-17 — `cmd(request, <C>Cmd)`: the route key is the service's own.**
The key sits inside each service's payload (`Notes.List(request_id,
route_key)`, as `Cmd.Service` carried it), so the driver cannot supply it
without parsing the service's union — the coupling the wrapper removes. The
component reads it and returns it in each `Answer`; the driver passes only
the host-minted request id (the app's own id field is ignored, as before).
Amends D-H7-7's signature.

**D-H7-18 — Engine-driving gates stay in the driver; service logic tests move.**
`gnotes.rs` drives host-im's `Engine` headlessly (`Boundary`, `window`,
`prof`, `ir::flatten`): it is a driver+service integration gate, and moving it
into `svc-notes` would link the engine into a second archive. It stays,
reaching the service the way any app does — plus, for its corpus-containment
oracle, the service's listing logic as a LIBRARY: `svc-notes` is
`staticlib + rlib` with its contract symbols behind a default `contract`
feature; host-im depends on it `default-features = false`, so the driver's
archive carries the (Rust-mangled, scan-exempt) logic and none of the
`hematite__svc_notes__*` symbols — measured: 0 in `libhost_im.a`, all in
`libsvc_notes.a`, scan clean over 2 archives. The one shared-state hazard —
each archive's `OnceLock` staging the fixture — is closed by making staging
existence-idempotent per process. Amends D-H7-8's "every g*.rs moves".

**D-H7-19 — `[world] interfaces_dir`: one `platform/interfaces/` for every
world of a repo.** Same shape as `cargo_root`; default stays `<world>/interfaces`.

**Interim, to be undone at P9:** host-dom's exhaustive `Cmd` match gains one
`other => unimplemented(…)` arm for service wrappers so the workspace keeps
compiling; DOM notesviewer is non-functional until `svc-notes-dom`.

**Measured on the real host:** the engine hands the shim an incref'd copy of
the wrapper shell — `release_cmds` deep-frees every drained payload with the
list, so the component's decref balances the copy and the original goes with
the list. The `Service`-era `notes_calls` collection, its engine arm and the
text protocol are gone; typed answers route under the service's key with the
host-minted request as `route`'s parameter, exactly as before. A wake courier
(`SERVICE_WAKES` + `take_wakes` in the pump) is in place for P5's first
asynchronous service. roc's warnings-fail discipline caught the one app-side
slip (a shadowed name).

## P4 decisions (2026-09-09) — the first asynchronous service

**D-H7-20 — `complete(token) -> RocList<Completion<E>>`: a wake yields zero or
more completions, in order.** A child's reader thread coalesces wakes (a burst
of lines costs one), and at EOF it must deliver the last lines AND the exit —
the invariant the whole process split exists for. One completion per wake
loses that tail; a coalesced no-op wake has no representation. General for
every later async service (dbx rows then done, net chunks). Amends D-H7-7.

**D-H7-21 — A spliced union needs two or more variants, full stop.** P0's
allowance for "one single-field variant" was wrong for the shim: glue
unwraps a single-variant union to its payload, so the named Rust type the
shim dispatches on does not exist at all. `Spawn` had one command; its second
is `Stop(job, route_key)` — kill the child — a real, small capability its own
doc reasons about, answered like any signal death (`Exited(seq, -1)`).
Rejected: two shapes of `Run` (an artificial split); waiting on an upstream
glue change (stalls every one-command service).

**Measured on the real host:** the stream is typed — `SpawnEvent.Lines(seq,
List(Str))` batches, `Exited(seq, code)`, `Failed(reason)` — and reaches the
app through `HostCtx.wake` → `take_wakes` → `on_wake` → `route`, with the
window nudged through the same `EventLoopProxy` the SQL worker uses
(`set_service_nudge`); headless gates just pump. The engine's `JobStream`,
`pump_jobs` and its per-frame `Poll` mode for children are gone (audio still
polls). The reader thread now OWNS the child and reaps it at EOF, so the exit
is never reported before the last line is taken. `im-spawn` passes unchanged
in what it asserts: order, a killed job's partial output, a failed start
answered. Tokens are `Box<u64>` job ids, allocated per wake in the component
and freed in `complete` (D9) — the first time that rule runs for real.

## P5 decisions (2026-09-09) — SQLite leaves the host

**D-H7-22 — `HostCtx.measure_text`: a driver capability for a service that
sizes text.** dbx sizes every column with the driver's exact font advances
(D8: under `virtual` the tree only carries the window, so widths cannot come
from layout), and those metrics live in `render-wgpu`'s `FontContext` — not
reachable from a service archive without dragging wgpu in. `HostCtx` gains
`measure_text(ptr, len, font) -> i32` (units); the service calls it from its
worker thread per cell, exactly where the old host pass measured. `HostCtx`
is D8's capability struct — P8 already planned `register_group` on it — and
D21's soundness-only rule stays about the roc runtime vtable. Rejected: the
driver post-processing `DbxEvent.Rows` (a dbx arm back in the driver);
app-side `Env.text_width` (the router has no `Env`; 20k measurements per
result in Roc).

**D-H7-23 — A service with a vendored native is reached ONLY through its
contract; no gate oracle links it as a library.** The notes pattern
(D-H7-18) broke here the way the scan predicted: `svc-dbx` as a
`default-features = false` dependency of host-im put bundled SQLite in the
driver's archive too — 280 `_sqlite3_*` collisions, H0c exactly — and would
have put SQLite in every world's driver, defeating the plan's exit. So the
driver links no part of it: the dbx gate seeds its fixture through a
`dbx-exec` gate hook (SQL via `ROC_SOLID_DBX_EXEC`, run by the service on the
database it opens from `ROC_SOLID_DBX`), and gets expected rows by
dispatching a real `Dbx.Fetch` through the shim and waiting on the wake. The
link proof (`dbx-link`) runs inside the service via its gate hook — the first
real use of D-H7-8's hook. Measured after: 0 `_sqlite3_*` in `libhost_im.a`,
280 in `libsvc_dbx.a`, scan clean over 4 archives.

**Within scope, decided by the code:** `CopyRow` was dead (no app, the gate
dropped row-copy) and is gone with `copy_row`/`LAST_SQL`; `Dbx`'s second
variant is `Tables(request, key)` — the app's first query was the
`sqlite_master` string, `dbx::tables()` already existed, and it answers as a
one-column `Rows` through the worker like any query; `DbxEvent := [Rows(…),
Failed(reason)]` replaces `Event.Rows` + the `Typed("error: …")` back
channel; the clipboard moved to the driver's own `clipboard.rs` (it was never
SQLite's); the `sqlite` Cargo feature and `rusqlite` left host-im entirely.
Wake delivery is NOT part of `pump_commands` after all: the window delivers
on its `Wake` event and a headless gate when it chooses — the shape
`deliver_completions` had, which the gates count on. P4's spawn gate pumps
both.

## P6 decisions (2026-09-09) — the network leaves the host

**D-H7-24 — The behaviour-script FORMAT is its own crate, `crates/spec`
(`roc-solid-spec`).** The plan moved `spec/canned.rs` into the service and
left `spec/runner.rs` in the driver without saying where the parser they
both read goes. Neither side can link the other: the driver must carry no
part of `svc-net` (P6's exit is 0 rustls in `libhost_im.a`), and a service
cannot depend on the driver crate. So the parser, the `Command` vocabulary
and the two payload envelopes (`decode_response`, `decode_error`, formerly
`pub(crate)` in the runner) are a dependency-free crate both link — the same
split `crates/ir` already makes for the tree. `http.rs` (the request format)
went INTO `svc-net` as the plan said; `svc-net-dom` (P9) will link `svc-net`
as a library for it, which is fine — it is the wasm world's transport, not a
driver.

**D-H7-25 — Harnesses reach the registry through a `net-ctl` gate hook.**
The runner and the http/net gates reason about requests by NAME
(`http:send:<key>`): count, peek, resolve, resolve-stale, reject. The
registry is the service's now and the driver links none of it, so every verb
is a gate-hook call — the command in `ROC_SOLID_NET_CTL`, the answer in the
file `ROC_SOLID_NET_CTL_OUT` names — behind a typed `netctl` module in the
driver. A harness's answer is INJECTED into the transport's channel and woken
like a socket's, so `pump_service_wakes` delivers it and the app cannot tell
a script from a network (the D2 property, now enforced by the seam rather
than by convention). Two consequences the gates surfaced: the registry is
process-wide rather than per-`Engine` (the arrival gate retires the other
engine's arrivals first, via a `forget` verb), and a source can be flipped
between engines because the service re-reads `--api`/`--spec`/env per
request. Rejected: linking `svc-net` into the driver as a library oracle
(D-H7-23's lesson; 3602 rustls symbols would be in every world's driver).

**D-H7-26 — `complete` delivers EVERY answer; the app's stale guard
decides.** The first cut filtered a drained answer through `retire` and
dropped it when the request was no longer pending — which is exactly what a
harness-resolved request is (resolve pops it), so `im-http`'s answer and
reject gates saw nothing. The old driver never filtered: under R6-1 the host
cannot read the model to know what is current, so a superseded answer
crosses in full under its own (older) id and the app drops it by comparison
— `resolve_stale` exists to stage precisely that. The service keeps that
rule; `retire` is bookkeeping (pending → gone; superseded → counted).

**Within scope, decided by the code:** `Net := [Send(key, req), Replace(key,
req)]` — the `replaces : U64` flag was the whole decision and a variant says
it (`Cmd.alongside`/`Cmd.supersedes` gone); `NetEvent := [Response({
status, body }), Failed(reason)]` with status-0 failures typed, and conduit
flattens both back to `{ status, body }` in one place (`Response.roc`) so
its pages keep one branch; `Source::Held` names "no backend" (requests stay
pending for a harness) instead of `Option<Net>`; the window's `run_at_api`
collapsed into `run_at`, and the service nudge is installed for EVERY
windowed app (it had been inside the `if api` arm — dbx in a window relied on
the mouse). `conduit-spec` is unverifiable on b07d7e (the compiler segfaults
on `apps/conduit`, recorded before P6); the runner's verbs are the seam
`im-http` exercises. An a7d4d3 build was tried and rejected as evidence: it
does not accept the P2 sources (`[Before, After, Same]`).

## P7 decisions (2026-09-09) — audio leaves the host, the first env block

**D-H7-27 — hematite tells a driver's build what the world wires:
`HEMATITE_SERVICES` (sorted wiring keys) and `HEMATITE_WORLD`, exported to
every cargo invocation.** The first env block exposed a gap: `Env.audio`
exists only in a world that wires `svc-audio`, and the driver's `Env` struct
literal must name exactly the fields the generated abi has — so one driver
serving two worlds needs world-conditional code. A `build.rs` in the driver
turns the list into `cfg(hematite_service = "<key>")` (with
`rustc-check-cfg`), and the code that fills `audio` or reads `playing`
compiles only where it can. Rejected: a per-world driver Cargo feature (a
second wiring table to keep in sync); `--cfg` through rustflags (fingerprints
every crate in the graph per world); a generated `env!` constructor macro
(does not cover reads). Cost: outside hematite nothing is wired, so
`cargo clippy`/`cargo test` see the un-cfg'd driver against whichever abi is
on disk — `lint` and `test-host` compose the default world first.

**D-H7-28 — A world that wires MORE is its own directory
(`platform/audio/`), not a `world-*.toml` variant of clay; the audio gate
runs against it.** `world-*.toml` variants were for Cargo feature sets that
leave the Roc contract alone. Wiring a service changes the contract
(`Audio`, `AudioEnv`, `Env.audio` and the glue), and the composed platform
is what apps build against — `im-check` composes clay once and fans out with
`IM_HOST_READY`, so a variant composing into clay's directory mid-suite would
break every neighbour. So audio is `platform/audio/world.toml` (clay's
driver and services plus `svc-audio`), composed by its own `audio-host`
recipe into its own directory, and `svc-audio` is linted and tested against
that abi. The old OFF gate ("a `Play` in a host without audio is answered,
not vanished") has nothing left to test — `Cmd.Audio` does not exist where
the service is not wired; the compiler answers — so `im-audio` gates the
crossing instead: every command reaches the service (`audio-handled`),
nothing is reported unknown, and `env.audio.playhead` shows the `Seek` and
`playing` the `Pause` in the same frame. `AudioEnv` gained `playing : Bool`
because the driver's live redraw (`audio_live`) needs it and an app wants it
anyway; Record/StopRecording stay in `im-audio-on`, which opens the
microphone. Asked and answered before building (the fork: default world
too, or the audio world only; wake-on-timer or a `playing` field).

## P8 decisions (2026-09-09) — the document engine leaves the host

**D-H7-29 — `register_group` borrows the scene for the CALL; the driver keeps
its own copy.** D-H7-11 had the driver borrow a page's draw list by pointer
between `group` and `drop`/`close`, to keep a Rust allocation from crossing
archives. Two facts changed the shape without changing the rule. The
allocator finding (P0): every archive's `__rust_alloc` is one first-wins
symbol, so one allocator serves the binary and a cross-archive free is not
the hazard it was drawn as. And the renderer's `SceneGroup` holds an
`Arc<Scene>`, so a driver holding a raw pointer would clone per frame or
change the renderer. The old `docsvc` already cloned once at `group`; the
callback does the same — `register_group(*const Scene, generation) -> i32`
copies into `hostres` and mints the handle from `imgref`'s counter (one id
space with images, so a handle names exactly one resource; the component
cannot reach that counter, which is why the driver mints). The component
stays the owner of its scene; the driver frees only what it allocated. The
rule stands, the borrow is shorter.

**Within scope, decided by the code:** typed per verb as the plan sketched,
with the wire's escaping gone; `Registry` as two boxed callbacks so
`docs.rs` is testable without a driver (the tests use a counting fake);
`Docs` in a `thread_local!` (every contract call is on the runtime thread,
and hayro's document need not be `Send`); `svc-doc` wired in clay — every
world — because a document engine has no device to open (unlike audio,
D-H7-28) and clay is the baseline every gate app binds to; the P10 per-app
worlds are where it is absent. In-repo evidence is `im-doc` (the fixture app
drives every verb the release protocol needs through the shim and the
driver's group table is full exactly between Group and Drop) plus the moved
table tests; the plan's "reader gates" belong to the nomadic repo's reader
app, which still speaks `Cmd.Service("doc")` and must be re-pointed there —
recorded as open.

## Still open (raised, not decided)

- Whether `platform/signals` is retired later (a separate decision; `just
  check` still runs its gates).
- `roc:test/quiesce` (D20) across several effect sources — first real chance is
  `platform/clay` with dbx + net + spawn in flight; not a gate of this pass.
- The nomadic reader (`../nomad/nomadic`) speaks `Cmd.Service("doc", …)` and
  parses text; it needs re-pointing at `Cmd.Doc`/`DocEvent` in its own repo,
  against a world that wires `svc-doc` (clay does). Its `test(doc)` and
  cropped-page gates are the reader-side evidence P8 named.
- `conduit-spec` (308/308) re-run the moment a toolchain builds `apps/conduit`
  again (`im-check-known-red` retries the build; the spec is one recipe more).

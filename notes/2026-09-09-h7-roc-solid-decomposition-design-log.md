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

## Still open (raised, not decided)

- Whether `platform/signals` is retired later (a separate decision; `just
  check` still runs its gates).
- `roc:test/quiesce` (D20) across several effect sources — first real chance is
  `platform/clay` with dbx + net + spawn in flight; not a gate of this pass.

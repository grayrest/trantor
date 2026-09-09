# Plan: H7 — decompose roc-solid's platform-im into hematite components

> **Status: P0 DONE 2026-09-09 (fixture `im-services` + spike `h7-wasm-inputs`
> green); P1 next.** P0 overturned two decisions — wasm32 staging (merge, not
> multiple inputs; D-H7-9 revised) and allocator-shim ownership (driver first
> + scan check; D-H7-13) — both confirmed 2026-09-09, see the design log's
> "P0 findings". Design log:
> [`notes/2026-09-09-h7-roc-solid-decomposition-design-log.md`](../notes/2026-09-09-h7-roc-solid-decomposition-design-log.md)
> (D-H7-1…12). Closes the PARTIAL H7 gate of
> [`2026-09-04-hematite-v1.md`](2026-09-04-hematite-v1.md). Toolchain pinned at
> `~/.bin/roc` = `roc-b07d7e-rebased-main`, `RustGlue-b07d7e-rebased-main.roc`.

Two repos change: **hematite** (the tool: `path`, splice markers, the service
contract + generated shim, `HostCtx`, wasm32 build) and **roc-solid** (the
consumer: `platform/<world>/`, six service crates, two driver worlds, per-app
worlds). Phases alternate so that every roc-solid step lands on a tool feature
already green in hematite's own suite.

## Exit (the whole plan)

1. `platform/clay` composes host-im + all six services; `just im-check` passes
   every gate it passes today, every gate invoked exactly as today.
2. `Cmd.Service` / `Event.Service` no longer exist; every service is a typed
   `<Svc>.Cmd` / `<Svc>.Event` (+ `<Svc>.Env` for audio).
3. Per-app worlds: `platform/colorhunt` (driver only), `platform/dbx` (dbx),
   `platform/conduit` (net), `platform/notesviewer` (notes). **`nm` on
   colorhunt's binary shows 0 `_sqlite3_*` and 0 `rustls` symbols**, by
   composition — host-im has no `sqlite`/`net`/`audio`/`pdf` features left.
4. `platform/dom` builds `apps/notesviewer` for wasm32 through `hematite build`
   and runs in the browser (`just dom-app`); `dom-recon` green.
5. hematite's golden suite green (existing 16 + `im-services` + the wasm spike
   fixture); zero-warning builds in both repos; `just check` green.

## Manifest additions (hematite)

```toml
# world.toml
[components.host-im]
kind = "driver"
path = "../../crates/host-im"          # NEW: default is components/<name>/
exports = ["Element", "Env", "Cmd", …]  # NEW for drivers: Roc modules it ships
                                        #   (crates/host-im/roc/*.roc), copied
                                        #   verbatim except marked splices
provides_runtime = true

[components.svc-notes]
kind = "host"
path = "../../crates/svc-notes"
exports = ["notes"]                     # the interface it implements (as today)

[wiring]
notes = "svc-notes"
```

```toml
# interfaces/notes/interface.toml
module = "Notes"                        # ships Notes.roc: the COMMAND union
event_module = "NotesEvent"             # NEW: ships NotesEvent.roc (P0: one
# env_module = "AudioEnv"               #   nominal per module); audio only
```

Wrapper variant / field name = the interface's module name: `Notes(Notes)`,
`Notes(NotesEvent)`, `audio : AudioEnv`; the app writes
`Cmd.Notes(Notes.List(0, "k"))`. Compose-time check (P0): a spliced union must
have ≥2 variants, or one variant with exactly one field (glue unwraps a
single-variant union and mis-types a multi-field payload).

Driver modules carry the splice markers; the block is replaced whole:

```roc
Cmd := [
    Log(Str),
    …core variants…
    ## @hematite(cmd)
    ## @end
]
```

`Env := { …, ## @hematite(env) … ## @end }` splices `audio : Audio.Env,` and
the driver's `hematite__host_im` frame assembly calls each env component.

## The service contract (generated into `abi/src/services.rs`)

Every host component with a `cmd` interface exports, `extern "C-unwind"`,
mangled `hematite__<sanitize(component)>__<fn>`:

| symbol | signature | when |
| --- | --- | --- |
| `init` | `(*const HostCtx)` | once, before the first frame; the component keeps the ctx |
| `cmd` | `(request: u64, route_key: RocStr, cmd: <Svc>Cmd) -> RocList<Answer>` where `Answer { route_key, event: <Svc>Event }` | each drained wrapper command; sync answers returned |
| `complete` | `(token: *mut c_void) -> Completion { request, route_key, event: <Svc>Event }` | on the runtime thread after `ctx.wake(id, token)` |
| `env` | `() -> <Svc>Env` | once per frame, env components only |
| `gate` | `(name: RocStr, argv: RocList<RocStr>) -> i32` | argv dispatch; −1 = not mine |

`HostCtx` (C-ABI, D8): `{ component_id: u32, wake: extern "C" fn(u32, *mut c_void),
register_group: …, release_group: … }` — the last two are the `hostres` borrow
protocol (D-H7-11), added in P8. The generated shim gives the driver:

- `services::dispatch(cmd: &abi::Cmd, request, route_key) -> Option<Vec<(String, abi::Event)>>`
  — one arm per component, wrapping the answers back into `Event::<Svc>(…)`;
  `None` for a core variant (the driver's own arms handle it).
- `services::on_wake(component_id, token) -> (request, route_key, abi::Event)`.
- `services::env_fields() -> impl Iterator<(name, value)>` — drivers splice it
  into their `Env` builder.
- `services::gate(name, argv) -> Option<i32>`.
- `services::init(ctx_factory)`.

Rules: no Rust-heap value crosses (D-H7-11); every boundary `C-unwind`
(H0d); Roc values built on the runtime thread only.

## Phases

### P0 — spikes (hematite; measure first) ✅ 2026-09-09

`tests/golden/im-services/` — imview-slice + two services (`svc-echo` sync,
`svc-tick` async from a thread) + an env block — green: wrapper unions cross
both ways (≥2 variants), `HostCtx.wake` → runtime-thread `complete`, env
block, gate chain; allocator shims measured as one first-wins symbol per link
(D-H7-13). `spikes/h7-wasm-inputs/` — multiple wasm inputs collide in both
forms; the rooted merge links and runs (D-H7-9 revised). Findings in the
design log.

### P1 — tool

`manifest`: `path`, driver `exports`, interface `cmd/event/env`.
`resolve`: component dirs by path; `archive_order` unchanged; new
`services: Vec<Service { component, module, cmd, event, env }>`.
`codegen`: driver modules copied from `<path>/roc/` with splice blocks filled;
never write `Cargo.toml`/`src` into an authored driver (`authored_host` +
`path` ⇒ hematite emits only the abi crate and the workspace); `abi/src/
services.rs` + `HostCtx`; workspace members by path.
`build`: `--target wasm32` pipeline (per-component `cargo rustc --target
wasm32-unknown-unknown` with the `dom-host` env overrides, `llvm-ar x` wasm
members per component, then ONE `wasm-ld -r --whole-archive <driver>
--no-whole-archive <each component's contract members> <component archives>`
→ `targets/wasm32/host.wasm` — the P0-measured merge; contract members found
with `llvm-nm` from the symbols hematite itself mangled; `exports:` emitted
from the driver's declared wasm exports); `scan` via `llvm-nm` for wasm
archives. `resolve`: driver FIRST in `archive_order` (D-H7-13); `scan`: the
three `___rustc*` shim symbols leave the Rust-mangled exemption and a
`#[global_allocator]` outside the driver is refused.
Exit: 16 fixtures + `im-services` green; `hematite build` on the wasm spike.

### P2 — roc-solid baseline, zero services extracted (behaviour-identical)

- `git mv platform-im/*.roc crates/host-im/roc/`; `git mv platform/ platform/signals/`
  (its 8 recipes + `examples/counter`, `examples/todo` re-pointed).
- `platform/clay/world.toml`: driver `host-im` via `path`, `frameworks` per
  `im-sysroot`'s list (AppKit… Security), no services yet; `Cmd.roc`/`Event.roc`/
  `Env.roc` get empty splice blocks.
- abi rename (D-H7-10): `crates/abi` deleted, `http.rs` → parked in host-im's
  `net.rs` until P6; `hematite_abi` at every import.
- 191 apps: `platform "../../platform-im/main.roc"` → `"../../platform/clay/platform/main.roc"`
  (sed; depth varies per dir).
- Justfile: `im-host`/`im-glue`/`im-sysroot` → one `im-host` that runs
  `hematite build platform/clay` (the `IM_HOST_READY` short-circuit and the
  atomic install kept); `dom-host` untouched until P9 (it breaks at P3's first
  union change — accepted only *between* P3 and P9, recorded in the plan).
Exit: `just im-check` all gates pass; `just check` green; colorhunt binary
byte-for-byte irrelevant but `nm` unchanged (still 280 sqlite — the flag).

### P3 — `svc-notes` (sync)

`crates/svc-notes` ← `notes.rs`, `gnotes.rs`; `interfaces/notes/Notes.roc`
(`Cmd := [List(U64, Str), Read(U64, Str, Str)]`, `Event := [Listing(List(Str)),
Text(Str), Failed(Str)]`). notesviewer's two call sites; the `notes` arm and
`notes_calls` leave the engine. `serve.py` keeps answering `/@service/notes`
for the DOM until P9. Exit: `im-notesviewer`, `notesviewer-probe` green.

### P4 — `svc-spawn` (streaming)

`Spawn.Cmd := [Run(U64, Str, List(Str))]`, `Spawn.Event := [Line({ seq, last, ok, body })]`;
`JobStream` polling moves behind `complete` (the child's reader thread wakes).
Exit: `im-spawn`.

### P5 — `svc-dbx` (async + the exit's sqlite)

`Dbx.Cmd := [Fetch(U64, Str, Str), CopyRow(U64)]`, `Dbx.Event := [Rows({…}), Failed(Str)]`
— `Fetch`/`CopyRow`/`Rows` leave the core (`im-g3/todo`, `im-g4/async`,
`im-g6/nested`, `im-map`, `apps/dbx` rewrite). `dbx.rs` (minus clipboard,
which stays), `worker.rs` (`wake` → `ctx.wake`), `gdbx.rs` move; the
`sqlite` feature and `rusqlite` are **deleted** from host-im. Exit: `im-dbx`,
`im-dbx-link`, `im-layout` dbx gates; host-im's Cargo.toml has no sqlite.

### P6 — `svc-net` (async + canned)

`Net.Cmd := [Http(U64, Str, Str)]`, `Net.Event := [Response({ status, body })]`;
`net.rs`, `tasks.rs`, `spec/canned.rs`, `abi/http.rs`, `gnet.rs`, `gtext.rs`,
`ghttp.rs` move; `Source::{Live, Canned}` stays inside the component, chosen
by argv exactly as today (`--api`, `--spec`); the `net` feature leaves
host-im. `spec/runner.rs` (the behaviour script) stays in the driver and
reaches the canned source through the gate hook. Exit: `im-net`, `im-http`,
`conduit-spec` (308/308).

### P7 — `svc-audio` (the env block)

`Audio.Cmd := [Play(Str), Pause, Seek(F64), Record(Str), StopRecording]`,
`Audio.Env := { playhead : F64, mic_level : F32 }` spliced into `Env`;
`sync_playhead` becomes `hematite__svc_audio__env`. `im-audio/audio.roc`
reads `env.audio.playhead`. The `audio` feature leaves host-im. Exit: `im-audio`.

### P8 — `svc-doc` (resources)

`Doc.Cmd`/`Doc.Event` typed per verb (`Open(U64, Str)`, `Size(U64, U32, U32)`,
`Blocks…`, `Group…`, `Drop…`, `Close…` → `Opened({doc, pages})`, `Sized`,
`Blocks(List({…}))`, `Group({handle, generation})`…). `HostCtx` gains
`register_group(handle, *const Scene, generation)` / `release_group(handle)`;
`hostres::GROUPS` holds borrowed pointers the component owns (D-H7-11);
`crates/doc` becomes `svc-doc`'s dependency; the `pdf` feature leaves host-im.
Exit: the reader gates (`test(doc)` oracle, cropped-page) green.

### P9 — `platform/dom`

host-dom as a second `authored_host` driver via `path` (same splice-marked
modules? **No** — the modules are host-im's; `platform/dom` reuses them by
declaring host-im's Roc dir as a `kind = "roc"` component `im-contract`
exporting the 55 modules, so both drivers share one contract text). Components
`svc-notes-dom`, `svc-net-dom` (JS `fetch` transport, wasm32). `dom-host`/
`dom-app` → `hematite build platform/dom --target wasm32`. Exit: exit item 4.

### P10 — per-app worlds + the `nm` exit; docs

Four worlds; four apps re-pointed; `Cmd.Service`/`Event.Service` deleted;
`hematite-v1.md` H7 → COMPLETE with the measured `nm` counts; this plan's
status flipped; design-log "Still open" updated.

## Risks

- **R-H7-1 nested-union crossing — CLEARED (P0).** Wrapper unions cross both
  ways with named per-service Rust types, given ≥2 variants (compose check).
- **R-H7-2 wasm32 multi-input — NEGATIVE (P0).** roc links wasm inputs
  `--whole-archive`; per-component inputs collide on std. The merge recipe
  above is the measured working shape (D-H7-9 revised, pending).
- **R-H7-7 allocator shim first-wins (P0).** `__rust_alloc` is one plain-
  external v0-mangled symbol per archive, first-wins across the link; the
  scan's Rust-mangled exemption is wrong for it when a `#[global_allocator]`
  exists. D-H7-13 (driver first + scan check) closes it; P2 measures mimalloc
  reachability on host-im.
- **R-H7-3 gate churn.** 191 apps re-pointed (P2) and ~25 files change variant
  spelling (P5/P6/P7). Mechanical, but `im-check` under load already trips the
  60 s cap; use `ROC_TIMEOUT=300` for the one clean reading after each phase.
- **R-H7-4 `roc glue` hazards on the composed platform.** The `Env` record
  gains a nominal field (`Audio.Env`); a local alias naming an imported nominal
  segfaults `roc check` (Cmd.roc's own note) — splice inline, never via alias.
- **R-H7-5 cross-allocator frees** (D-H7-11). No test catches a violation
  until it corrupts; the rule is enforced by the shim's signatures (only Roc
  values and `*mut c_void` tokens cross) and by review at P8.
- **R-H7-6 DOM window** (P3–P9): `dom-host` does not compile between the first
  union change and P9. Bounded by the phase order; recorded here.

# Plan: H7 — decompose roc-solid's platform-im into trantor components

> **Status: COMPLETE 2026-09-09 (P0–P10).** Follow-up 2026-09-10: eight
> worlds, one per distinct wiring and each named for what it wires —
> `clay` (nothing), `dbx`, `net`, `notes`, `doc`, `spawn`, `audio`, `dom`
> (D-H7-39). A world
> composes into `target/trantor/<world>/` — generated files are separated
> from source by construction, and a world directory holds only its
> `world*.toml` (D-H7-38). clay is
> now THE BASE and wires nothing (D-H7-36) — 178 of its 184 app entries
> named no service yet linked SQLite, rustls and hayro; the six that do
> moved to `platform/gate-<service>`. A clay app is 17 MB with none of the
> three, against 27 MB with all of them. Also: a driver's
> `exports`/`frameworks` default into every world that wires it, so seven world
> files stopped carrying the same two lists (D-H7-35), and `just world-clean`
> drops the ~800 MB of composed output. P10: four per-app worlds
> (`platform/colorhunt`, `platform/dbx`, `platform/conduit`,
> `platform/notesviewer`) wire exactly what each app names; `Cmd.Service`/
> `Event.Service` are gone from both drivers; the exit is measured by `just
> world-nm` (colorhunt = `libhost_im.a` alone; dbx = + `libsvc_dbx.a`; conduit = + `libsvc_net.a`; notesviewer = + `libsvc_notes.a`; clay = all five services — and every world's `libhost_im.a` has 0 `_sqlite3_*`, 0 rustls, 0 hayro, 0 cpal symbols). A driver may ask for the shim in every world
> (`services_shim`, D-H7-33). P9: the DOM
> driver is the world `platform/dom` — host-im's contract shipped by a
> `kind = "roc"` component and its driver.toml by `contract_from` (D-H7-30),
> `svc-notes-dom`/`svc-net-dom` over the browser's `fetch` through the
> driver's `dom_svc_*` externs, `trantor build --target wasm32` in place of
> `dom-host`/`dom-app`, and the counter, notesviewer and the request fixture
> running in a browser; the shim learned wasm32 (D-H7-32); the size-correct
> knob is ON — fat LTO gave every component its own heap; thin does not
> (D-H7-41). P8: the document
> engine left the host — `svc-doc`, `Doc`/`DocEvent` typed per verb, page draw
> lists published through `HostCtx.register_group` (D-H7-11 as built: borrowed
> for the call, D-H7-29); 0 hayro symbols in the driver; `im-doc` gates the
> crossing in-repo, the nomadic reader re-points in its own repo. P7: audio left the host —
> `svc-audio` with the first env block (`Env.audio`), wired only in its own
> world `platform/audio` (D-H7-28); one driver serves worlds whose contracts
> differ through `TRANTOR_SERVICES` → `cfg(trantor_service = "…")`
> (D-H7-27); 0 cpal/symphonia/flac symbols in clay's driver. P6: the network left
> the host — `svc-net` (`Net := [Send, Replace]`, `NetEvent := [Response,
> Failed]`), the behaviour-script format as `crates/spec` shared by the
> driver's runner and the service's canned loader (D-H7-24), harnesses reach
> the registry through the `net-ctl` hook (D-H7-25), every answer delivered
> and the app's guard decides (D-H7-26); 0 rustls/ureq symbols in the
> driver's archive; `conduit-spec` unverifiable on b07d7e (compiler segfault,
> already recorded). P5: SQLite left the host
> — `svc-dbx`, `HostCtx.measure_text` (D-H7-22), no library oracle for a
> native (D-H7-23); 0 `_sqlite3_*` in the driver's archive. P4: `spawn` is the first
> asynchronous service out — wake courier, list-valued `complete` (D-H7-20),
> `Stop` (D-H7-21). P3: `notes` is the
> first service out (`crates/svc-notes`, `platform/interfaces/notes`), typed
> end to end through the generated shim on the real host; D-H7-17/18/19 in
> the log. P2: roc-solid's
> platform is the trantor world `platform/clay` (driver `crates/host-im`,
> zero services extracted); `im-check` 132/132 on `~/.bin/roc` (b07d7e) with
> two gates quarantined for compiler segfaults that predate nothing here
> (`known_red`, roc-solid's UPSTREAM-ISSUE note); clippy + cargo test green;
> the migration alone measured 133/134 on the previous pin. Extra tool work
> P2 forced: `cargo_root` (D-H7-14), `--platform-only`, features as cargo
> flags, write-if-changed, framework `Versions/` links. P0
> overturned two decisions — wasm32 staging (merge, not multiple inputs;
> D-H7-9 revised) and allocator-shim ownership (driver first + scan check;
> D-H7-13) — both confirmed, see the design log's "P0 findings". P1 landed the
> tool: `path`, service interfaces, splice markers, the generated
> `abi/src/services.rs`, driver-first archives, the wasm32 merge pipeline and
> the flag-based wasm scan; fixtures `im-services` and `wasm-host` are the
> proofs. Design log:
> [`notes/2026-09-09-h7-roc-solid-decomposition-design-log.md`](../notes/2026-09-09-h7-roc-solid-decomposition-design-log.md)
> (D-H7-1…12). Closes the PARTIAL H7 gate of
> [`2026-09-04-trantor-v1.md`](2026-09-04-trantor-v1.md). Toolchain pinned at
> `~/.bin/roc` = `roc-b07d7e-rebased-main`, `RustGlue-b07d7e-rebased-main.roc`.

Two repos change: **trantor** (the tool: `path`, splice markers, the service
contract + generated shim, `HostCtx`, wasm32 build) and **roc-solid** (the
consumer: `platform/<world>/`, six service crates, two driver worlds, per-app
worlds). Phases alternate so that every roc-solid step lands on a tool feature
already green in trantor's own suite.

## Exit (the whole plan)

1. `platform/clay` composes host-im + all six services; `just im-check` passes
   every gate it passes today, every gate invoked exactly as today.
2. `Cmd.Service` / `Event.Service` no longer exist; every service is a typed
   `<Svc>.Cmd` / `<Svc>.Event` (+ `<Svc>.Env` for audio).
3. Per-app worlds: `platform/colorhunt` (driver only), `platform/dbx` (dbx),
   `platform/conduit` (net), `platform/notesviewer` (notes). **`nm` on
   colorhunt's binary shows 0 `_sqlite3_*` and 0 `rustls` symbols**, by
   composition — host-im has no `sqlite`/`net`/`audio`/`pdf` features left.
4. `platform/dom` builds `apps/notesviewer` for wasm32 through `trantor build`
   and runs in the browser (`just dom-app`); `dom-recon` green.
5. trantor's golden suite green (existing 16 + `im-services` + the wasm spike
   fixture); zero-warning builds in both repos; `just check` green.

## Manifest additions (trantor)

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
    ## @trantor(cmd)
    ## @end
]
```

`Env := { …, ## @trantor(env) … ## @end }` splices `audio : Audio.Env,` and
the driver's `trantor__host_im` frame assembly calls each env component.

## The service contract (generated into `abi/src/services.rs`)

Every host component with a `cmd` interface exports, `extern "C-unwind"`,
mangled `trantor__<sanitize(component)>__<fn>`:

| symbol | signature | when |
| --- | --- | --- |
| `init` | `(*const HostCtx)` | once, before the first frame; the component keeps the ctx |
| `cmd` | `(request: u64, cmd: <Svc>Cmd) -> RocList<Answer>` where `Answer { route_key, event: <Svc>Event }` — the route key is the service's own payload field, returned per answer (D-H7-17) | each drained wrapper command; sync answers returned |
| `complete` | `(token: *mut c_void) -> RocList<Completion { request, route_key, event: <Svc>Event }>` — zero or more per wake, in order (D-H7-20) | on the runtime thread after `ctx.wake(id, token)` |
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

### P0 — spikes (trantor; measure first) ✅ 2026-09-09

`tests/golden/im-services/` — imview-slice + two services (`svc-echo` sync,
`svc-tick` async from a thread) + an env block — green: wrapper unions cross
both ways (≥2 variants), `HostCtx.wake` → runtime-thread `complete`, env
block, gate chain; allocator shims measured as one first-wins symbol per link
(D-H7-13). `spikes/h7-wasm-inputs/` — multiple wasm inputs collide in both
forms; the rooted merge links and runs (D-H7-9 revised). Findings in the
design log.

### P1 — tool ✅ 2026-09-09

As built: `manifest.rs` (`path`, `component_dir`/`module_path` — a
component's Roc modules live in `<dir>/roc/` or `<dir>/`; `Interface.kind =
"service"` + `event_module`/`env_module`; `Driver.wasm_exports`),
`resolve.rs` (`Service`, driver-first `archive_order`, the ≥2-variant check),
`splice.rs`, `services.rs` (the shim generator), `codegen.rs` (driver modules
spliced; nothing written into a `path` driver; `features` refused with
`path`), `scan.rs` (`#[global_allocator]` guard; wasm via `llvm-readobj`
flags; `rust_eh_personality` singleton), `wasm.rs` (the merge; roots = members
defining `trantor__<c>__*`). A service with events must export `complete`
even when synchronous (the contract is uniform). Exit met: 16 prior fixtures
+ `im-services` + `wasm-host` green, zero warnings, 13 unit tests.

Original plan text follows.
`manifest`: `path`, driver `exports`, interface `cmd/event/env`.
`resolve`: component dirs by path; `archive_order` unchanged; new
`services: Vec<Service { component, module, cmd, event, env }>`.
`codegen`: driver modules copied from `<path>/roc/` with splice blocks filled;
never write `Cargo.toml`/`src` into an authored driver (`authored_host` +
`path` ⇒ trantor emits only the abi crate and the workspace); `abi/src/
services.rs` + `HostCtx`; workspace members by path.
`build`: `--target wasm32` pipeline (per-component `cargo rustc --target
wasm32-unknown-unknown` with the `dom-host` env overrides, `llvm-ar x` wasm
members per component, then ONE `wasm-ld -r --whole-archive <driver>
--no-whole-archive <each component's contract members> <component archives>`
→ `targets/wasm32/host.wasm` — the P0-measured merge; contract members found
with `llvm-nm` from the symbols trantor itself mangled; `exports:` emitted
from the driver's declared wasm exports); `scan` via `llvm-nm` for wasm
archives. `resolve`: driver FIRST in `archive_order` (D-H7-13); `scan`: the
three `___rustc*` shim symbols leave the Rust-mangled exemption and a
`#[global_allocator]` outside the driver is refused.
Exit: 16 fixtures + `im-services` green; `trantor build` on the wasm spike.

### P2 — roc-solid baseline, zero services extracted (behaviour-identical)

Tool prerequisite landed first (D-H7-14, `src/cargo.rs`, fixture
`tests/golden/cargo-root`): `[world] cargo_root = "../.."` builds `path`
components inside roc-solid's own workspace with the world's abi patched in
(`trantor-abi = "0.0.0"` in each crate; root `[patch.crates-io]` default to
`platform/clay/abi`). The Justfile finds the tool at `~/.bin/trantor`
(`TRANTOR` override; D-H7-15).

- `git mv platform-im/*.roc crates/host-im/roc/`; `git mv platform/ platform/signals/`
  (its 8 recipes + `examples/counter`, `examples/todo` re-pointed).
- `platform/clay/world.toml`: driver `host-im` via `path`, `frameworks` per
  `im-sysroot`'s list (AppKit… Security), no services yet; `Cmd.roc`/`Event.roc`/
  `Env.roc` get empty splice blocks.
- abi rename (D-H7-10): `crates/abi` deleted, `http.rs` → parked in host-im's
  `net.rs` until P6; `trantor_abi` at every import.
- 191 apps: `platform "../../platform-im/main.roc"` → `"../../platform/clay/platform/main.roc"`
  (sed; depth varies per dir).
- Justfile: `im-host`/`im-glue`/`im-sysroot` → one `im-host` that runs
  `trantor build platform/clay` (the `IM_HOST_READY` short-circuit and the
  atomic install kept); `dom-host` untouched until P9 (it breaks at P3's first
  union change — accepted only *between* P3 and P9, recorded in the plan).
Exit: `just im-check` all gates pass; `just check` green; colorhunt binary
byte-for-byte irrelevant but `nm` unchanged (still 280 sqlite — the flag).

**Outcome ✅ 2026-09-09.** As built, beyond the list above: the compiler port
to `~/.bin/roc` (D-H7-16: `List.sort_with`'s `[Before, After, Same]`,
`U64.order_relative_to`, two F64 annotations in `grid-finance`); `just
im-host` = `trantor build platform/clay --world <w> --platform-only` with one
world file per former feature set; probe scripts under `notes/probes` follow
the platform; `known_red` quarantines `im-underline` and `im-id-apps`
(b07d7e segfaults, note in roc-solid). `just check` also fails at
`env-twins`, which fails identically at the previous HEAD (a stale twin
table) — not P2's. Results: `im-check` 132/132, clippy clean, `cargo test
--workspace` green, 191 apps type-check.

### P3 — `svc-notes` (sync)

`crates/svc-notes` ← `notes.rs`, `gnotes.rs`; `interfaces/notes/Notes.roc`
(`Cmd := [List(U64, Str), Read(U64, Str, Str)]`, `Event := [Listing(List(Str)),
Text(Str), Failed(Str)]`). notesviewer's two call sites; the `notes` arm and
`notes_calls` leave the engine. `serve.py` keeps answering `/@service/notes`
for the DOM until P9. Exit: `im-notesviewer`, `notesviewer-probe` green.

**Outcome ✅ 2026-09-09.** As built: `crates/svc-notes` (`notes.rs` logic +
`contract.rs`; `contract` feature; library API for the gate's oracle),
`platform/interfaces/notes/{Notes,NotesEvent}.roc` (`interfaces_dir`),
`gnotes.rs` stays in host-im (D-H7-18), the engine's wrapper arm + answer
routing + wake courier, `main()`'s gate chain, host-dom's interim catch-all,
notesviewer typed. `trantor__svc_notes__*` only in `libsvc_notes.a`; scan
clean over 2 archives; `im-notesviewer` + `notesviewer-probe` green.

### P4 — `svc-spawn` (streaming)

`Spawn.Cmd := [Run(U64, Str, List(Str))]`, `Spawn.Event := [Line({ seq, last, ok, body })]`;
`JobStream` polling moves behind `complete` (the child's reader thread wakes).
Exit: `im-spawn`.

**Outcome ✅ 2026-09-09.** As built: `crates/svc-spawn` (`spawn.rs`: the
reader thread owns the child, coalesces wakes, reaps at EOF; `contract.rs`),
`platform/interfaces/spawn/{Spawn,SpawnEvent}.roc` — `Spawn := [Run(U64,
Str, Str, List(Str)), Stop(U64, Str)]` (D-H7-21), `SpawnEvent := [Lines(U64,
List(Str)), Exited(U64, I32), Failed(Str)]`; `complete` returns a list
(D-H7-20); the engine's `JobStream`/`pump_jobs` are gone and
`pump_service_wakes` + the window nudge carry every async service from here.
Scan clean over 3 archives; `im-spawn` green.

### P5 — `svc-dbx` (async + the exit's sqlite)

`Dbx.Cmd := [Fetch(U64, Str, Str), CopyRow(U64)]`, `Dbx.Event := [Rows({…}), Failed(Str)]`
— `Fetch`/`CopyRow`/`Rows` leave the core (`im-g3/todo`, `im-g4/async`,
`im-g6/nested`, `im-map`, `apps/dbx` rewrite). `dbx.rs` (minus clipboard,
which stays), `worker.rs` (`wake` → `ctx.wake`), `gdbx.rs` move; the
`sqlite` feature and `rusqlite` are **deleted** from host-im. Exit: `im-dbx`,
`im-dbx-link`, `im-layout` dbx gates; host-im's Cargo.toml has no sqlite.

**Outcome ✅ 2026-09-09.** As built: `crates/svc-dbx` (`dbx.rs`, `worker.rs`,
`contract.rs`; `Dbx := [Fetch, Tables]`, `DbxEvent := [Rows, Failed]`;
`CopyRow` dead and gone), `HostCtx.measure_text` for column widths
(D-H7-22), the gate reaching the database only through the service
(D-H7-23: `dbx-exec` hook + a real `Dbx.Fetch` through the shim; the driver
links no part of `svc-dbx`), the clipboard as the driver's own module, the
`sqlite` feature and `rusqlite` out of host-im. **Measured: `libhost_im.a`
has 0 `_sqlite3_*` symbols; `libsvc_dbx.a` 277**; scan clean over 4
archives; `im-dbx`, `im-dbx-link`, `im-layout`, `im-g3/4/6` green.

### P6 — `svc-net` (async + canned)

`Net.Cmd := [Http(U64, Str, Str)]`, `Net.Event := [Response({ status, body })]`;
`net.rs`, `tasks.rs`, `spec/canned.rs`, `abi/http.rs`, `gnet.rs`, `gtext.rs`,
`ghttp.rs` move; `Source::{Live, Canned}` stays inside the component, chosen
by argv exactly as today (`--api`, `--spec`); the `net` feature leaves
host-im. `spec/runner.rs` (the behaviour script) stays in the driver and
reaches the canned source through the gate hook. Exit: `im-net`, `im-http`,
`conduit-spec` (308/308).

**Outcome ✅ 2026-09-09 (conduit-spec blocked upstream).** As built:
`crates/svc-net` (`net.rs` with `Source::{Live, Canned, Held}` + `inject`,
`tasks.rs`, `canned.rs`, `http.rs` from `crates/ir`, `contract.rs`),
`platform/interfaces/net/{Net,NetEvent}.roc` — `Net := [Send(Str, Str),
Replace(Str, Str)]` (the `replaces` flag became the variant; `Cmd.alongside`
/ `Cmd.supersedes` gone), `NetEvent := [Response({ status, body }),
Failed(Str)]`; the source is chosen per request from `--api` / `--spec` /
`ROC_SOLID_NET_API` / `ROC_SOLID_NET_SPEC`, else HELD for a harness. The
behaviour-script FORMAT (parser, vocabulary, envelopes) is `crates/spec`
(`roc-solid-spec`), linked by the driver's runner and the service's canned
loader (D-H7-24); the runner and the http/net gates drive the registry
through the `net-ctl` gate hook (`crates/host-im/src/netctl.rs`, D-H7-25);
`complete` delivers every answer and the app's stale guard decides
(D-H7-26). The engine lost `tasks`/`net`/`deliver_answers`/`answer`; the
window's `run_at_api` collapsed into `run_at` (the service nudge is now
installed for every windowed app); `Cmd.Http`/`Event.Http` left the core;
conduit's pages route `Event.Net` through `apps/conduit/Response.roc`;
host-dom lost its HTTP ops (P9 rebuilds them as `svc-net-dom`). The `net`
feature and `ureq` are out of host-im. **Measured: `libhost_im.a` has 0
rustls and 0 ureq symbols; `libsvc_net.a` 3602/1284**; scan clean over 5
archives; `im-net`, `im-http` green. `conduit-spec` cannot run: b07d7e
segfaults compiling `apps/conduit` (`notes/2026-09-09-UPSTREAM-ISSUE-b07d7e-
segfaults.md` in roc-solid) — the runner's every registry verb is the same
`net-ctl` seam `im-http` exercises (count, peek, resolve, resolve-stale,
reject, stale-ignored), which is the evidence available until the toolchain
moves.

### P7 — `svc-audio` (the env block)

`Audio.Cmd := [Play(Str), Pause, Seek(F64), Record(Str), StopRecording]`,
`Audio.Env := { playhead : F64, mic_level : F32 }` spliced into `Env`;
`sync_playhead` becomes `trantor__svc_audio__env`. `im-audio/audio.roc`
reads `env.audio.playhead`. The `audio` feature leaves host-im. Exit: `im-audio`.

**Outcome ✅ 2026-09-09.** As built: `crates/svc-audio` (`audio.rs` whole,
`contract.rs`: `cmd` performs on arrival with no event module, `env` returns
the block, `audio-gate` hook = the old ON gate, `audio-handled` for the app
gate), `platform/interfaces/audio/{Audio,AudioEnv}.roc` — `Audio := [Play(Str),
Pause, Seek(F64), Record(Str), StopRecording]`, `AudioEnv := { playhead : F64,
mic_level : F32, playing : Bool }` (`playing` added: the driver's live-redraw
reads it, D-H7-28). Wired ONLY in `platform/audio/world.toml`, a world
directory of its own (D-H7-28; `world-audio.toml` deleted); the fixture
targets it and `im-audio` composes it inside the suite. The driver reads the
block under `cfg(trantor_service = "audio")`, which trantor's build now
makes possible (D-H7-27: `TRANTOR_SERVICES`/`TRANTOR_WORLD` exported to
cargo, host-im's `build.rs` turns them into cfgs). Gone from the driver:
`Transport`, `perform_transport`, `sync_playhead`, `player`/`recorder`,
`Boundary.{playhead,mic_level}`, the `audio` feature and its three deps,
`audio_gate`; `Cmd.Play…StopRecording` and `Env.{playhead,mic_level}` left
the core (host-dom's arms and fields too). `lint`/`test-host` check
`svc-audio` against its own world's abi. **Measured: clay's `libhost_im.a`
has 0 cpal-crate, 0 symphonia, 0 flac symbols; `libsvc_audio.a` 583/6460/1027**;
scan clean over 6 archives; `im-audio` (typed crossing + env block) and
`im-audio-on` (link/enumerate) green; `im-check` 132/132.

### P8 — `svc-doc` (resources)

`Doc.Cmd`/`Doc.Event` typed per verb (`Open(U64, Str)`, `Size(U64, U32, U32)`,
`Blocks…`, `Group…`, `Drop…`, `Close…` → `Opened({doc, pages})`, `Sized`,
`Blocks(List({…}))`, `Group({handle, generation})`…). `HostCtx` gains
`register_group(handle, *const Scene, generation)` / `release_group(handle)`;
`hostres::GROUPS` holds borrowed pointers the component owns (D-H7-11);
`crates/doc` becomes `svc-doc`'s dependency; the `pdf` feature leaves host-im.
Exit: the reader gates (`test(doc)` oracle, cropped-page) green.

**Outcome ✅ 2026-09-09.** As built: `crates/svc-doc` (`docs.rs` — the table
of open documents, typed per verb, with a `Registry` of two callbacks for the
page groups; `contract.rs`), `platform/interfaces/doc/{Doc,DocEvent}.roc` —
`Doc := [List, Open, Close, Size, Blocks, Crop, Chars, Outline, Chapter,
Group, Drop]` each `(request_id, route_key, …)`, `DocEvent := [Listing,
Opened, Closed, Dropped, Sized, Blocks, Crop, Chars, Outline, Chapter, Group,
Failed]` carrying the records the app used to parse out of text (the wire's
`\n`/`\t` escaping is gone with the wire). trantor's `HostCtx` gained
`register_group(*const Scene, generation) -> handle` / `release_group(handle)`
and `services::init(…, Option<Groups>)`; the driver's callbacks mint the
handle from `imgref`'s counter and COPY the borrowed scene into `hostres`
(D-H7-29). Wired in clay (every world). Gone from the driver: `docsvc.rs`,
`DOC_SERVICE`, `doc_calls`, `Engine.docs`, the `pdf` feature, `roc-solid-doc`
and the `zip` dev-dep; `Cmd.Service` now reports every name unknown (P10
deletes it). New: `tests/integration/im-doc/doc.roc` + `im-doc` (list → open
→ size/blocks/group → drop → close through the shim, the group table full
exactly in between; `tests/assets/doc/sample.pdf`). **Measured: clay's
`libhost_im.a` has 0 hayro / 0 roxmltree / 0 zip symbols**; scan clean over
6 archives; `im-doc` green; `im-check` 133/133. The reader gates
the plan named (`test(doc)` oracle, cropped-page) live in the nomadic repo
with the reader app; re-pointing that app at `Cmd.Doc` is that repo's change
and is NOT done here — `crates/doc`'s own oracle tests are unchanged and
green.

### P9 — `platform/dom`

host-dom as a second `authored_host` driver via `path` (same splice-marked
modules? **No** — the modules are host-im's; `platform/dom` reuses them by
declaring host-im's Roc dir as a `kind = "roc"` component `im-contract`
exporting the 55 modules, so both drivers share one contract text). Components
`svc-notes-dom`, `svc-net-dom` (JS `fetch` transport, wasm32). `dom-host`/
`dom-app` → `trantor build platform/dom --target wasm32`. Exit: exit item 4.

**Outcome ✅ 2026-09-09.** As built: `platform/dom/world.toml` — driver
`host-dom` (`crates/host-dom/driver.toml`: `authored_host`, `wasm_exports`,
`contract_from = "../host-im/driver.toml"` for the shared `requires`/
`provided`), the 53 contract modules from `crates/host-im/roc` as the
pure-Roc component `im-contract` (spliced like a driver's, D-H7-30),
`svc-notes-dom` (`List`/`Read` as POSTs to the dev server's
`/@service/notes`) and `svc-net-dom` (`svc-net`'s registry and request format
behind `default-features = false`, the browser's `fetch` as the transport, a
superseded request ABORTED there). A DOM service reaches the browser through
the driver's `dom_svc_http` / `dom_svc_call` / `dom_svc_abort` externs,
handing over the request and a callback; the applier performs it and calls
`dom_http_done` / `dom_service_done` exactly as before, and the driver
forwards the answer to the callback filed under that request — the JS is
untouched. host-dom drains wakes at the top of every frame and routes typed
completions with the others; its wrapper catch-all is gone (every wrapper
this world's `Cmd` has is a wired service). One app, two platforms: an app
dir carries `main.roc` and `dom.roc` (`examples/im-counter`,
`apps/notesviewer`, `tests/integration/im-http`), `dom-entries` keeps the
pairs identical below the header, `dom-exports` keeps the applier's calls in
the export list (which is how two latent applier bugs surfaced: a
`pushInput` call C2 had removed, and `dom_dispatch` missing from the export
list — every click had trapped since the plain export-everything build went
away). trantor: `contract_from`, roc-component splicing, `--app <file.roc>`,
`[world] wasm_size_correct` (D-H7-31), `tagged::build` in every service
world's abi, plain-`C` contract declarations on wasm32, first-wins on the
merge (D-H7-32). `lint`/`test-host`/`dom-recon` check the DOM crates against
`platform/dom/abi`. **Measured in a browser (Claude's pane over `serve.py`):
the counter counts, notesviewer lists `notes/` through `svc-notes-dom`, the
request fixture shows the dev server's answers through `svc-net-dom`;
host.wasm 4.7 MB → app.wasm 444 KB (counter, after wasm-opt); `im-check`
133/133 unchanged.** The size-correct build traps on the first service
completion and is off (D-H7-31); `just dom-notesviewer` is the demo.

### P10 — per-app worlds + the `nm` exit; docs

Four worlds; four apps re-pointed; `Cmd.Service`/`Event.Service` deleted;
`trantor-v1.md` H7 → COMPLETE with the measured `nm` counts; this plan's
status flipped; design-log "Still open" updated.

**Outcome ✅ 2026-09-09.** As built: `platform/{colorhunt,dbx,conduit,
notesviewer}/world.toml` — clay's driver with exactly the services the app
names (none; `dbx`; `net`; `notes`), each composing into its own directory
through a `<app>-host` recipe so the suite's parallel gates can compose them
beside clay; `apps/<app>/main.roc` (and conduit's three test apps) name
their world; `just world-nm` measures every driver archive. host-im asks for
the shim in every world (`services_shim = true`, D-H7-33) so colorhunt's
service-less world compiles the same driver. `Cmd.Service` and
`Event.Service` are deleted: the engine's arm and `unknown_services`, host-
dom's arm, `Answer`, `inflight`/`completions`, `complete_service`,
`service_event` (its `Op::Service` stays — it is how `svc-notes-dom` reaches
the dev server, and `service_name_ok` now guards it in `dom_svc_call`), and
the T1 rig's claim 4 (`gt1::gate_service`). **Measured (`just world-nm`):
colorhunt = `libhost_im.a` alone; dbx = + `libsvc_dbx.a`; conduit = + `libsvc_net.a`; notesviewer = + `libsvc_notes.a`; clay = all five services — and every world's `libhost_im.a` has 0 `_sqlite3_*`, 0 rustls, 0 hayro, 0 cpal symbols.** `im-check` 133/133, the four per-app worlds composing inside the parallel suite under the workspace build lock (D-H7-34). The roadmap note
in trantor's design log records H7 as complete with these numbers; there is
no `trantor-v1.md`.

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

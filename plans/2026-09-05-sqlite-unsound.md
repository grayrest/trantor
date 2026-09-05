# Plan: `roc:sqlite-unsound` — borrowed-slice cursor fold, two engines

A new SQLite namespace for hematite, built on tower-platform's **H3 spike**
(`spike(host): internal-iteration cursor fold with borrowed-slice rows`), not
basic-cli's `Sqlite.roc`. Two interchangeable host backends — **rusqlite** and
**turso** — provide one identical interface; turso adds a single Roc-visible
extension leaf (a Roc-defined SQL scalar function). This is P3's "Sqlite → its
own world" plus the H5 substitution thesis at SQL scale.

The `-unsound` in the name is load-bearing and honest: the row cells are
**zero-copy borrows** into the live engine buffer, which is only sound while a
borrowed cell is not *retained* past its reducer call. Upstream Roc is receptive
to **clone-on-incref** (an incref of a borrowed value deep-copies it), which
closes the one remaining hazard. Until it lands, the namespace ships unsound and
says so in its name.

## Decisions (grill 2026-09-05)

- **S1 — New design over the tower H3 spike**, not basic-cli's decoder-combinator
  `Sqlite.roc`. The spike is the reference: an internal-iteration fold with
  borrowed-slice rows.
- **S2 — Name `roc:sqlite-unsound`.** Sound only under clone-on-incref (not yet
  upstream); the suffix owns it.
- **S3 — Core primitive: generic-state `sql_fold!`, single path.** No bytes-only
  second primitive.
- **S4 — Fully encapsulated; no app-visible handle, no `ctx`.** Two leaves —
  `sql_fold!` (reads, internal iteration) and `sql_exec!` (writes/DDL, host runs
  to Done). `db` is a `Str`; the host caches connections by db. tower's `ctx`
  coeffect and read/write-cache split are dropped.
- **S5 — Borrowed-slice rows.** Row = `List(SqlValue)`,
  `SqlValue := [Null, Integer(I64), Real(F64), Text(Str), Blob(List(U8))]`;
  Text/Blob cells are zero-copy seamless slices (rc==0 static backing) into the
  live cell buffer, valid only for one reducer call. Port tower's hand-written
  `borrow.rs`.
- **S6 — The unsoundness is exactly one nameable hazard:** a reducer that
  *retains* a borrowed cell into the returned `state`. Encapsulation (S4) removes
  every other escape surface; internal iteration bounds the row lifetime
  structurally. clone-on-incref closes the retain hazard.
- **S7 — Green gate on non-retaining folds; record-decode is the documented
  clone-on-incref target.** Every `verify.sh` fold retains nothing (aggregate to a
  number, fold into serialized `List(U8)`, predicate-count over `Text`, concat
  into one growing `Str`) — correct and green today. `fold -> List(record-with-Str)`
  ships as a **documented, not-asserted** example that flips from garbage to
  correct when clone-on-incref merges.
- **S8 — Two interchangeable host components, sole-vendor each.** `rusqlite-host`
  (bundles libsqlite3 C) and `turso-host` (pure Rust), each providing
  `roc:sqlite-unsound`, wired per-world, `nm`/`ar`-checked (H0c). Substitution
  proof: the same non-retaining fold app runs on both by swapping one wiring line.
- **S9 — turso extensions that stay off the Roc surface:** vector search is
  **base SQL** (turso accepts `vector_distance_cos(…)`, rusqlite rejects — the
  substitution's negative half); encryption and streaming **replication** are
  **host-side config** (key / replica URL from env), invisible to Roc.
- **S10 — One turso-only Roc leaf: `turso_register_scalar!`.** Register a Roc
  closure as a SQL scalar function, callable from `SELECT` and from
  `CREATE TRIGGER … BEGIN … END` bodies — the "Roc-handled" feature. Same
  erased-callable mechanism as the fold reducer. turso has no update/commit/WAL
  hooks, so classic change-triggers aren't available; aggregates and virtual
  tables are deferred.
- **S11 — Feasibility de-risked before writing this plan.** hematite's pinned
  compiler (84812227) has `immortal_locals.zig` (rc==0 immortal slices); its glue
  already generates the erased-callable ABI (`RocErasedCallableFn` in
  imview-slice); tower borrows straight from turso cells. SQ0 proves both on
  hematite's own stack.

## Interfaces

`roc:sqlite-unsound` (module `Sql`; both hosts):

```roc
SqlValue := [Null, Integer(I64), Real(F64), Text(Str), Blob(List(U8))]
sql_exec! : { db : Str, sql : Str, params : List(SqlValue) } => Try({}, Str)
sql_fold! : { db : Str, sql : Str, params : List(SqlValue) }, state, Box((state, List(SqlValue) -> state)) => Try(state, Str)
```

`roc:turso` (module `Turso`; turso-host only):

```roc
turso_register_scalar! : Str, Box((List(SqlValue) -> SqlValue)) => Try({}, Str)
```

The reducer receives cells **positionally**; a by-name `Row { columns, cells }`
wrapper is userland sugar and a clean future variant (it needs the fold to
surface column names). Bindings are positional `List(SqlValue)` (`?1, ?2`).

## Gates

Each ends with a self-checking `verify.sh`; commits are gated on the full suite
staying green plus the gate's own checks. Fixtures under `tests/golden/`.

### SQ0 — Substrate: borrowed-slice immortality + host-calls-Roc-closure

The two load-bearing mechanisms, proven in isolation before any DB.

- Port tower's hand-written `borrow.rs` (`borrowed_str`/`borrowed_bytes`: a
  seamless slice whose `bytes` aims at a caller buffer and whose alloc-ptr aims at
  a shared static rc==0 block) into the hematite abi crate as a **hand-written
  module the glue never regenerates** (R-SQ5).
- Micro-fixture A: a host leaf returns a `borrowed_str` over a host-owned buffer;
  Roc reads it, concatenates it (forcing a copy-out), and the borrow is never
  freed or mutated. Proves rc==0 immortal slices work on 84812227 (R-SQ1).
- Micro-fixture B: a host leaf takes a `Box(a -> b)` and invokes it from the host
  (erased callable), returning the result. Proves host-calls-boxed-Roc-closure in
  a **host component** (not just the driver), the pattern both `sql_fold!` and
  `turso_register_scalar!` ride.
- **Exit:** both micro-fixtures build and run; the borrowed value survives a
  copy-out and the process exits clean (no free of static data, alloc-gauge
  balanced for the non-borrowed allocations); the erased callable returns the
  right value.

### SQ1 — `roc:sqlite-unsound` + `rusqlite-host` (the fold)

- Interface `Sql` (SqlValue, `sql_exec!`, `sql_fold!`). `rusqlite-host`:
  `sql_exec!` prepares, steps to Done, finalizes; `sql_fold!` prepares, drives an
  internal iteration, builds each row's `List(SqlValue)` with Text/Blob cells
  **borrowed** from rusqlite's `ValueRef` (`&[u8]` valid until the next step),
  calls the Roc reducer, returns the folded `state`. Connection cache keyed by
  `db` string.
- Fixture world (rusqlite) + a fold app exercising **non-retaining** reducers over
  a seeded mixed-type table: sum an Integer column (`state : I64`), fold into a
  JSON `List(U8)` (Text escaped/copied on consume), predicate-count over Text,
  concat names into one `Str`. `sql_exec!` for CREATE/INSERT/DDL.
- **Exit:** every fold returns the correct value; resource/alloc balance holds;
  `verify.sh` asserts the four fold shapes.

### SQ2 — `turso-host`, the substitution proof

- `turso-host` provides the **same** `Sql` interface over turso's **blocking
  `turso_core`** (`stmt.step()` → `StepResult::Row`), *not* the async `turso`
  binding (no tokio/`block_on` in a sync platform). Text/Blob cells borrowed from
  turso cells (`borrowed_str(b.as_ptr(), …)`, as tower does). Connection cache by
  `db`.
- A turso world = the SQ1 fixture with one wiring line swapped
  (`sqlite = "turso-host"`). The **same fold app** runs on both.
- **Exit:** the fold app produces identical output on the rusqlite and turso
  worlds (swap the wiring, rerun); `nm`/`ar` confirm each world vendors only its
  own engine (H0c — a sqlite-less world links neither); the fixture SQL stays
  inside turso's supported subset (R-SQ3).

### SQ3 — turso vector + encryption demos (no Roc surface)

- **Vector (base SQL):** the turso world runs a nearest-neighbour query
  (`… ORDER BY vector_distance_cos(embedding, ?) LIMIT k`) through the base
  `sql_fold!`; the rusqlite world runs the same statement and returns an error —
  the substitution's negative half.
- **Encryption (host-side):** `turso-host` opens the db with a key from env; the
  fold reads correct rows while the on-disk `.db` is ciphertext.
- **Exit:** the vector query returns the expected ordering on turso and errors on
  rusqlite; the encrypted db's bytes on disk are not plaintext yet the fold reads
  correct rows; no Roc leaf added for either.

### SQ4 — `roc:turso` scalar UDF leaf

- `turso-host` provides `roc:turso` with `turso_register_scalar!`. Registering a
  Roc closure binds a SQL name; a later `sql_fold!`/`sql_exec!` whose SQL calls
  that name — in a `SELECT` and in a `CREATE TRIGGER` body — invokes the Roc
  closure (erased callable), which maps `List(SqlValue) -> SqlValue`.
- Fixture: register a Roc scalar (e.g. a checksum/normalize), call it from a
  query and from a trigger fired by an `INSERT`. The rusqlite world does **not**
  expose `roc:turso` (turso superset).
- **Exit:** the Roc scalar returns correct values from both a query and a trigger;
  the scalar's borrowed `SqlValue` args obey the same non-retain rule; the
  rusqlite world lacks the `Turso` module (compose/`nm` check); balance holds.

### SQ5 — Record-decode target + write-up + publish

- Ship `fold -> List({ … : Str })` as a **documented, not-asserted** example: it
  is garbage today and becomes correct under clone-on-incref, with the one-line
  change (drop the copy) and the assertion to enable, noted in place.
- `hematite publish` the rusqlite and turso worlds; `tier` them; a design-log note
  records S1–S11, the clone-on-incref dependency, and how to flip the record-decode
  assertion green when upstream merges.
- **Exit:** the documented example is present and clearly marked; both worlds
  publish; the note names the exact upstream change and the assertion it unblocks.

## Risks

- **R-SQ1 — rc==0 immortality of borrowed *heap* slices on 84812227.**
  `immortal_locals.zig` is present, but it may target compile-time-known locals,
  not a runtime seamless slice whose alloc-ptr's refcount word reads 0. If the
  compiler's generated incref/decref touches that word (frees / corrupts), the
  borrow doesn't work and the port stalls on clone-on-incref (or a compiler bump).
  **Settle at SQ0** — this is the go/no-go.
- **R-SQ2 — turso engine surface.** `turso_core` (blocking) is an internal crate;
  the public `turso` crate is async and tower used `turso_sdk_kit`. Confirm a
  blocking, borrow-exposing turso API is usable from a hematite host without a
  runtime. **Settle at SQ2.**
- **R-SQ3 — turso SQLite-compat gaps.** turso is a rewrite in progress; the
  substitution app's SQL must sit in turso's supported subset. Measure; keep the
  fixture SQL conservative.
- **R-SQ4 — Roc-in-VDBE re-entrancy.** `turso_register_scalar!`'s callback runs
  Roc *inside* a turso query step; the erased callable must be safe to invoke
  mid-execution, and its borrowed `SqlValue` args carry the retain hazard.
  **Settle at SQ4.**
- **R-SQ5 — hand-written `borrow.rs` vs glue regen.** The composer regenerates
  `abi/src/generated.rs` from RustGlue; `borrow.rs` must be a separate
  hand-written abi module that `compose` never emits or clobbers.

## Non-goals

Soundness (by name, pending clone-on-incref); a bytes-only fold (S3); app-visible
handles/cursors (S4); tower's `ctx` / response-cache / read-write split; named
bindings and a by-name `Row` in the core interface; turso aggregates and virtual
tables; Roc surfaces for CDC / replication / encryption (S9); the async `turso`
binding + `block_on`; basic-cli's `Sqlite.roc` verbatim and its two examples
migrating by URL.

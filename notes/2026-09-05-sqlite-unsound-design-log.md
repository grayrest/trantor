# `roc:sqlite-unsound` — design log (SQ0–SQ5)

A SQLite namespace built on tower-platform's H3 borrowed-slice cursor-fold spike,
with two interchangeable sole-vendor engines (rusqlite, turso) and one turso-only
Roc leaf. Plan: [`plans/2026-09-05-sqlite-unsound.md`](../plans/2026-09-05-sqlite-unsound.md).
Fixture: `tests/golden/sqlite-unsound/`.

## Decisions (S1–S11)

Grill-settled 2026-09-05 (see the plan for the full text):

- **S1** design over the H3 borrowed-slice spike, not basic-cli's `Sqlite.roc`.
- **S2** name `roc:sqlite-unsound` — sound only under clone-on-incref.
- **S3** core primitive: generic-state `sql_fold!`, single path.
- **S4** fully encapsulated; `sql_fold!`/`sql_exec!`, no handle, no `ctx`; `db` a
  Str, host-cached connections.
- **S5** borrowed-slice rows (`SqlValue [Null,Integer,Real,Text,Blob]`, Text/Blob
  zero-copy into the live cell buffer).
- **S6** the unsoundness is one hazard: retaining a borrowed cell into `state`.
- **S7** green gate on non-retaining folds; record-decode is the clone-on-incref
  target.
- **S8** two interchangeable host components, sole-vendor each.
- **S9** vector = base SQL, encryption + replication = host-side, no Roc surface.
- **S10** one turso leaf: `turso_register_scalar!`.
- **S11** feasibility de-risked at SQ0.

## What each gate proved / found

- **SQ0 — substrate (the go/no-go).** Ported tower's `borrow.rs` into the
  composer-emitted abi (`abi::borrow`, hand-written, never glue-clobbered). On
  hematite's pinned compiler (84812227): a borrowed Str used TWICE (incref once,
  decref twice) against a read-only rc==0 static backing survives — rc==0
  immortal works here (R-SQ1 cleared). And a host component invokes a boxed Roc
  closure through the erased-callable ABI (the fold/scalar path). `live=0`.
- **SQ1 — rusqlite fold.** `Box(state)` erases to `RocBox` at the ABI boundary,
  so the host is generic over the accumulator (imview's `Box(Model)` trick).
  Cells borrowed from rusqlite `ValueRef`. Four non-retaining reducers correct +
  `live=0`. A stray `{ s }` that retained the first cell produced garble
  mid-write — the retain hazard, live.
- **SQ2 — turso substitution.** Same interface over blocking `turso_core`
  (`turso_sdk_kit` 0.7). turso's `row_value(i)` returns an OWNED Value, so the
  fold holds the row's Values alive across the reducer call and borrows from them
  — same zero-copy, same unsound-if-retained, different lifetime source. Same app
  → byte-identical output on both engines; sole-vendor per app.
  **R-SQ2 finding:** turso pulls `iana_time_zone` (via chrono, in turso_core),
  which needs macOS CoreFoundation. roc only links platform-bundled frameworks,
  so build.sh generates a `macos-sysroot` symlinking the host SDK's libSystem +
  CoreFoundation (the `.framework` must be a REAL dir — roc's discovery skips
  symlinked entries). build.sh stages only the archives `main.roc` links.
- **SQ3 — turso extras, no Roc surface.** Vector = base SQL (turso orders by
  `vector_distance_cos`; rusqlite rejects `vector32`). Encryption = host-side,
  env-gated (`HEMATITE_TURSO_ENCRYPTION_HEXKEY`, aes256gcm): correct reads,
  ciphertext at rest (the control shows plaintext leaks into the WAL without the
  key).
- **SQ4 — `turso_register_scalar!`.** A Roc `List(SqlValue) -> SqlValue` closure
  registered as a turso SQL scalar (`register_external_scalar_function`, the
  closure pointer threaded through turso's `context: usize`), invoked from a
  SELECT and a TRIGGER body. **R-SQ4 finding:** the trampoline runs INSIDE the
  VDBE and re-enters Roc — works; the closure is BORROWED per call (null reuse)
  and retained for the process by the registry (`live=1` by design, not a leak).
- **SQ5 — target + publish.** `record-app` folds into `List({id, name})` and is
  the documented clone-on-incref target (below). Both worlds publish (each
  baseline ships only its engine; the turso baseline includes `Turso.roc`;
  test-only `report-host` is excluded); the sqlite world tiers as Tier 2 (host
  code).

## The clone-on-incref dependency (the one thing left unsound)

`record-app/main.roc` is the ergonomic decode the namespace aims at — fold rows
straight into `List(record)`. It **type-checks** (the API shape is right) but is
**not run**: `collect_person` retains each borrowed `name` cell into the
accumulator, which dangles today (the next step overwrites the cell buffer).

Upstream **clone-on-incref** (an incref of a borrowed value deep-copies it) makes
`List.append(acc, { id, name })` own the bytes with no code change. When it
lands:

1. drop the `roc check`-only guard on `record-app` in `verify.sh`'s SQ5 section,
2. build + run it, and
3. assert its output: `rows: 1:alice,2:amy,3:bob`.

Until then the green gate exercises only non-retaining folds (S7), and the
`-unsound` name carries the warning.

## Non-goals still open

A by-name `Row` in the core; named bindings; turso aggregates + virtual tables;
Roc surfaces for CDC / replication / encryption; the async `turso` binding;
basic-cli's `Sqlite.roc` verbatim.

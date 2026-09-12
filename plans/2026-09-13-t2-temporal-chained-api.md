# T2 — trantor-temporal, chained API

**Design log:** [`notes/2026-09-12-t1-temporal-release-design-log.md`](../notes/2026-09-12-t1-temporal-release-design-log.md).
T1's decisions D-T1-1 … D-T1-25 stand except where this file supersedes them;
new decisions continue at D-T2-1.

Repo: `../trantor-temporal`, clean at `643bee8`, gate green at 70 behaviours.

## Why

Roc style prefers chained calls. The current surface is all free functions
taking every option positionally — `Temporal.add!(jan31, dur, iso)?` — which
reads nothing like the rest of the platform's packages (`dir.join(x)`,
`file.read_utf8!()?`).

## Compiler capabilities — MEASURED, not assumed

Every line below was checked against the pinned compiler
(`release-fast-10e922df`) in a composed world. The probes are the reason this
plan is shaped the way it is; do not "simplify" past them.

| construct | result |
|---|---|
| `T := { … }.{ methods }` nominal over a record | works |
| `d.to_str()` / `d.add!(x)?.to_str!()?` chaining | works |
| `d.year` on a nominal record, from outside the module | works |
| `{ ..base, hour: 10 }` record spread | works |
| `{ ..nominal_value, day: 1 }` spread of a nominal | works |
| `T := { h : SomeOpaque }.{ methods }` wrapping a resource | works, and RUNS |
| `==` on a nominal | needs an `is_eq` method on the type |
| `{ hour: 10, minute: 40 }` — fields omitted | **type error** |
| `minute ? U8` — optional field | **parse error** |
| `{ hour : U8 }a` — open record | **parse error** |
| `minute : U8 = 0` — default value | **parse error** |
| `{ base & hour: 10 }` — old update syntax | **parse error** |
| methods on a record *alias* (non-nominal) | **not possible** |
| `T := Box(U64)` used transparently, or unwrapped | **no syntax found** |

Consequences that drive the design:

- Partial records do not exist. Anything "optional" is spelled by spreading a
  named zero value.
- A resource cannot carry methods directly; it must be wrapped in a one-field
  nominal record (`ZonedDateTime := { h : TemporalHost.ZonedDateTime }`).

## Decisions taken (D-T2-1 … D-T2-5)

1. **Values become nominal types with method blocks.** Fields stay readable
   from outside, so D-T1-2's round-trip property survives. `is_eq` is defined
   on each so `==` keeps working.
2. **Optional arguments are spread from a named zero.** `Temporal.time` is
   midnight; `Temporal.duration` is all-zeros. There is deliberately NO date
   zero — a date always names all three fields.
3. **There is no `with`.** `add`/`subtract` do arithmetic and take a record.
   Field replacement is already spelled by the language: `{ ..jan31, day: 1 }`
   spreads a nominal value, so `Temporal.plain_date({ ..jan31, day: 1 })` IS
   TC39's `with`. TC39 needs the method because a JS Temporal object is opaque
   and has no spread; Roc has one, and a second way to say it would be the
   only way to say it wrong.
4. **`Calendar` becomes a closed tag union**, not a resource. 16 canonical
   calendars, measured from temporal_rs 0.2.6: `Iso`, `Buddhist`, `Chinese`,
   `Coptic`, `Dangi`, `Ethioaa`, `Ethiopic`, `Gregory`, `Hebrew`, `Indian`,
   `IslamicCivil`, `IslamicTbla`, `IslamicUmalqura`, `Japanese`, `Persian`,
   `Roc`. There is no creation step and nothing to fail.
5. **`TimeZone` becomes its identifier string**, not a resource. The set is
   open (598 zones and tzdb grows), so a tag union is not available.
   `ZonedDateTime` is then the ONLY resource in the package.
6. **The host MUST memoise the identifier → `TimeZone` mapping.** This is not
   an optimisation; without it the change is a 6.3x regression on every zoned
   call. See below.

Measured basis for 4: a `Calendar` is 8 bytes and clones in ~1 ns — the
`Box(U64)` was buying refcounting for something smaller than a pointer to it.
A tag match into a static table costs nothing, so the tag union has no
resolve cost at all; it never parses a string.

Measured basis for 5 and 6. A `TimeZone` is NOT an identifier — it is
`IanaIdentifier(TimeZoneId { normalized: NormalizedId(170), resolved:
ResolvedId(0) })`, two resolved indices into the tzdb provider. Passing the
string across the ABI therefore discards resolution the type exists to cache.
Best of 5 runs, 300k iterations, against a ZONED operation (the earlier figure
in this plan compared a zone resolve against `PlainDate::add`, which uses no
zone, and was meaningless):

| | ns/op |
|---|---|
| a. `TimeZone` held resolved — what a resource gives you | 44.6 |
| b. resolve the identifier on every call | 280.0 |
| c. host-side cache, one hot zone | 56.5 |
| d. host-side cache, five zones rotating | 46.8 |

So (b) is 6.3x (a), and a cache lands within 5% of it. The user's model is "an
identifier the host can map back to the data"; the cache IS that mapping, and
the API keeps the plain-data property. Key on the identifier bytes and look up
WITHOUT allocating a `String` per call — the allocating form measured 104 ns
against 64 ns for `get(&str)`.

## Work

### 1. Interface — `interfaces/temporal/TemporalHost.roc` (127 lines)

- `Calendar :: Box(U64)` → `Calendar : [Iso, Buddhist, …]` (16 tags).
- `TimeZone :: Box(U64)` → `TimeZone : Str`.
- `ZonedDateTime :: Box(U64)` unchanged — still the one real resource.
- 17 of 34 leaves change signature. Leaf ORDER is load-bearing (glue names
  result types by first declaration, B7 finding 2) — do not reorder.
- `calendar_from_id!` / `time_zone_from_id!` lose their effect and their
  `Try` where nothing can fail; `calendar_from_id` stays as a pure fallible
  Str → tag mapping, because IXDTF parsing carries `[u-ca=hebrew]`.

### 2. Host — `components/temporal-host/src/lib.rs` (1008 lines)

- `cal()` and `tz()` resource accessors go; replace with a tag → `Calendar`
  match and a `RocStr` → `TimeZone` resolve.
- **Owned-argument rule (B0):** the zone identifier is now a `RocStr` argument
  on 17 leaves. Every one must decref exactly once on every path, including
  the error paths. This is the single highest-risk part of the change — two
  resource leaks were already found in T1 by exactly this class of mistake.
- Two of three resource types disappear, so `abi::resource::live()` drops to
  counting ZonedDateTime alone. Re-baseline b7's gauge expectation.

### 3. Pure layer — `components/temporal-lib/`

- `Temporal.roc` (505 lines): nominal `PlainDate`, `PlainTime`, `Duration`,
  and `ZonedDateTime := { h : TemporalHost.ZonedDateTime }`, each with a
  method block. Add `Temporal.time`, `Temporal.duration`, `plain_date`,
  `plain_time`, and `is_eq` per type.
- `Plain.roc` (225 lines): the pure helpers move into the method blocks;
  `duration_to_str` becomes `Duration.to_str`. The module may disappear.
- `Strftime.roc` (1002 lines): internal, unchanged, but its 160 `expect`s
  construct records directly and will need the new constructors.
- `Now.roc` (74 lines): returns the new types.

### 4. Gate — `verify.sh` (433 lines)

All 70 behaviours rewrite to the new surface. The EXPECTED values do not
change — that is the point of doing it this way round: if a pinned answer
moves, the rework broke something. Treat any diff as a defect until proven
otherwise.

### 5. Fixture and docs

- `tests/golden/b7-temporal` consumes the package; its app rewrites too.
- README: the user has rewritten §1–§3 to the target API. The rest of the file
  (Differences, Formatting, Now, Errors, Known upstream defects) still
  describes the old surface and must follow.

## Order

1 → 2 → gate green on the hosted layer alone → 3 → 4 → 5. Commit at each.
The gate is the oracle throughout; it is currently green and every step must
leave it green before the next starts.

## Risks

- **Leaf order.** Renaming a host result type silently breaks glue. Diff the
  generated `abi/src/generated.rs` before and after §1.
- **RocStr decref on 17 leaves.** See §2.
- **The Strftime expects.** 160 of the gate's 387 expects live there; they
  construct bare records today.
- **Scope.** This touches every file in the package. The gate's 70 pinned
  behaviours and the 598-zone sweep are what make it safe to attempt.

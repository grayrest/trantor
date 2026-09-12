# T1 — trantor-temporal for release

**Design log:** [`notes/2026-09-12-t1-temporal-release-design-log.md`](../notes/2026-09-12-t1-temporal-release-design-log.md).
Decisions are D-T1-1 … D-T1-13; this file does not re-argue them.

Repo: `../trantor-temporal` (sibling checkout, one commit, clean tree).

## Baseline state

Was blocked on trantor-cli's net split, which is now complete (`94bb2f4`,
`d2a357e`). `verify.sh` passes at `trantor-temporal@5c6e000` against
`trantor-cli@d2a357e`, so the pre-rewrite gate is green and any failure from
here is this work's.

## Target surface

### Layer 1 — hosted `TemporalHost` (D-T1-1)

`interfaces/temporal/interface.toml` gets `module = "TemporalHost"`; the file
becomes `interfaces/temporal/TemporalHost.roc`. 22 leaves, up from 14.

```roc
TemporalHost :: [].{
	ZonedDateTime :: Box(U64)
	TimeZone :: Box(U64)
	Calendar :: Box(U64)

	## ISO fields, always — on the way in and on the way out (D-T1-2).
	PlainDate : { year : I32, month : U8, day : U8 }
	PlainTime : { hour : U8, minute : U8, second : U8, millisecond : U16, microsecond : U16, nanosecond : U16 }
	Duration : {
		years : I64, months : I64, weeks : I64, days : I64,
		hours : I64, minutes : I64, seconds : I64, milliseconds : I64,
		microseconds : I64, nanoseconds : I64,
	}
	Unit : [Year, Month, Week, Day, Hour, Minute, Second, Millisecond, Microsecond, Nanosecond]
	Overflow : [Constrain, Reject]
	Disambiguation : [Compatible, Earlier, Later, Reject]
	Err : [OutOfRange(Str), Invalid(Str), Other(Str)]

	calendar_from_id! : Str => Try(Calendar, Err)
	calendar_id! : Calendar => Str
	time_zone_from_id! : Str => Try(TimeZone, Err)
	time_zone_id! : TimeZone => Try(Str, Err)

	date_add! : PlainDate, Duration, Calendar, Overflow => Try(PlainDate, Err)
	date_until! : PlainDate, PlainDate, Calendar, Unit => Try(Duration, Err)
	date_day_of_week! : PlainDate, Calendar => Try(U8, Err)
	date_from_str! : Str => Try(PlainDate, Err)

	zdt_from_epoch_ns! : I128, TimeZone, Calendar => Try(ZonedDateTime, Err)
	zdt_from_wall_clock! : PlainDate, PlainTime, TimeZone, Calendar, Disambiguation => Try(ZonedDateTime, Err)
	zdt_from_str! : Str => Try(ZonedDateTime, Err)
	zdt_epoch_ns! : ZonedDateTime => I128
	zdt_with_time_zone! : ZonedDateTime, TimeZone => Try(ZonedDateTime, Err)
	zdt_calendar! : ZonedDateTime => Calendar
	zdt_plain_date! : ZonedDateTime => PlainDate
	zdt_plain_time! : ZonedDateTime => PlainTime
	zdt_offset_seconds! : ZonedDateTime => I64
	zdt_to_str! : ZonedDateTime => Try(Str, Err)
	zdt_add! : ZonedDateTime, Duration, Overflow => Try(ZonedDateTime, Err)
	zdt_until! : ZonedDateTime, ZonedDateTime, Unit => Try(Duration, Err)
	zdt_start_of_day! : ZonedDateTime => Try(ZonedDateTime, Err)

	duration_from_str! : Str => Try(Duration, Err)
}
```

Declaration order is load-bearing: glue names a result type after the first
leaf that declares its shape and reuses it for every structurally identical one
(B7 finding 2). Put a comment at the top of `interface.toml` saying so, and do
not reorder leaves without rebuilding and re-reading `abi/src/generated.rs`.

### Layer 2 — shim `Temporal`, a new `components/temporal-lib/Temporal.roc`

Pure where it can be (D-T1-8). Re-exports every type above.

Pure:

```roc
date_to_str : PlainDate -> Str                    # "%Y-%m-%d", zero-padded
date_format : PlainDate, Str -> Str
date_parse : Str, Str -> Try(PlainDate, ParseErr)
time_to_str : PlainTime -> Str                    # "%H:%M:%S"
time_format : PlainTime, Str -> Str
time_parse : Str, Str -> Try(PlainTime, ParseErr)
duration_to_str : Duration -> Str                 # ISO 8601, "P1Y2M3D" / "PT0S"
duration_negate : Duration -> Duration
iso_day_of_week : PlainDate -> U8                 # Sakamoto; Monday = 1
ParseErr : [BadPattern(Str), BadInput(Str)]
```

Effectful, wrapping the leaves with the D-T1-4 defaults:

```roc
add! : PlainDate, Duration, Calendar => Try(PlainDate, Err)              # Constrain
add_with_overflow! : PlainDate, Duration, Calendar, Overflow => Try(PlainDate, Err)
subtract! : PlainDate, Duration, Calendar => Try(PlainDate, Err)         # negated duration
until! : PlainDate, PlainDate, Calendar => Try(Duration, Err)            # Day
until_in! : PlainDate, PlainDate, Calendar, Unit => Try(Duration, Err)
since! : PlainDate, PlainDate, Calendar => Try(Duration, Err)            # swapped args

zdt_from_wall_clock! : PlainDate, PlainTime, TimeZone, Calendar => Try(ZonedDateTime, Err)   # Compatible
zdt_from_wall_clock_with! : …, Disambiguation => Try(ZonedDateTime, Err)
zdt_add! / zdt_add_with_overflow! / zdt_subtract!                        # Constrain
zdt_until! / zdt_until_in! / zdt_since!                                  # Hour
zdt_equals! : ZonedDateTime, ZonedDateTime => Try(Bool, Err)             # epoch_ns + zone id + calendar id
zdt_format! : ZonedDateTime, Str => Try(Str, Err)
```

`zdt_equals!` compares instant *and* zone *and* calendar, which is Temporal's
`equals` — raw `zdt_epoch_ns!` equality is instant-only and is not the same
question.

### strftime directives (D-T1-9, D-T1-10)

Format and parse share the table. Literal `%` is `%%`.

| | |
|---|---|
| `%Y` `%y` | year 4-digit / 2-digit (parse: 00–68 → 2000s, 69–99 → 1900s, POSIX) |
| `%m` `%d` | month, day, zero-padded |
| `%e` | day, space-padded |
| `%b` `%B` | month name, abbreviated / full (parse: case-insensitive) |
| `%a` `%A` | weekday name (parse: validated against the computed weekday; mismatch is an error) |
| `%j` | day of year, 3 digits |
| `%H` `%I` `%p` | hour 24 / hour 12 / AM-PM |
| `%M` `%S` | minute, second |
| `%L` `%N` | milliseconds (3) / nanoseconds (9) |
| `%z` `%:z` `%Z` | offset `+0400` / `+04:00` / zone id — format only, zoned only |
| `%F` `%T` | `%Y-%m-%d` / `%H:%M:%S` |
| `%%` `%n` `%t` | literal `%`, newline, tab |

Parsing is strict: literals match exactly, the whole input must be consumed,
every field the pattern names must be present, no defaulting.

## Work

**M1 — hosted layer. DONE, verified.** Host compiles, symbol scan clean, and a
19-case exercise app over a real composed world confirms each fix: the Hebrew
zero-duration identity (`2024-1-31`, was `5784-5-21`), `OutOfRange(unknown
calendar)` with no duplicated kind in the message, Feb 30 rejected under
`Constrain`, `until` answering 359d or 11m 24d on demand, all four
disambiguation modes at the 2026-03-08 gap, and `+P1D` across the jump elapsing
82800000000000 ns while keeping the wall clock. Also settled empirically: the
flat-parameter ABI is correct for a 4- and 5-argument leaf carrying 10-field
records, matching every shipped host rather than `interface-stub`'s `Args`
form. Plus D-T1-13, found here.

Original M1 scope: `interface.toml`, `TemporalHost.roc`,
`components/temporal-host/src/lib.rs`.

- `rec_of` reads through `with_calendar(Calendar::ISO)` (D-T1-2, fixes #1).
- `time_zone_id!` and `zdt_to_str!` return `Try` and propagate rather than
  `unwrap_or_default()` (fixes #2).
- `Err` becomes `[OutOfRange, Invalid, Other]`; `to_err` classifies on
  `Display`'s kind prefix — `RangeError` → `OutOfRange`, `TypeError` →
  `Invalid`, anything else → `Other` — and strips that prefix from the payload
  message (D-T1-7, fixes #3 #4 #5 #6).
- Record validation keeps `PlainDate::try_new` (Reject); the `Overflow`
  argument is threaded to `add` only (D-T1-5).
- New leaves: `date_from_str!` (`PlainDate::from_utf8`), `zdt_from_str!`
  (`ZonedDateTime::from_utf8`, `OffsetDisambiguation::Reject` per D-T1-12),
  `duration_from_str!` (`Duration::from_utf8`), `zdt_from_wall_clock!`
  (`ZonedDateTime::from_partial` — *not* `PlainDate::to_zoned_date_time`, which
  has no disambiguation knob), `zdt_add!`, `zdt_until!`, `zdt_start_of_day!`,
  `zdt_calendar!`.
- `duration_rec`'s `i128 → i64` casts: `i64::try_from(…).map_err(…)?` rather
  than `as`, surfacing the truncation as an `Err` instead of wrapping.

**M2 — shim layer.** `components/temporal-lib/Temporal.roc`, new `[components.temporal-lib]`
(`kind = "roc"`, `exports = ["Temporal"]`) in `package.toml`, and
`exports = ["Temporal", "TemporalHost"]` under `[package]`.

Formatter and parser are the bulk of it and are pure — unit-testable without
composing a world.

**M3 — docs.** README: the two surfaces and why (mirror trantor-cli's README
shape), the strftime table, the D-T1-11 note that there is no `PlainDateTime`,
how to get from `Clocks` to a `ZonedDateTime` since "now" lives in the
baseline, and an `https://` link to this package's own repo rather than
`http://` to trantor's. Add a LICENSE file.

**M4 — gate.** `verify.sh` keeps its three existing checks and drops the
`2>/dev/null` on the build so a failure states its cause. Add:

- error paths: `calendar_from_id!("nosuchcal")` → `OutOfRange`, and a bad zone
  id, asserting the tag rather than just failure.
- the DST cases measured in the design log: `+P1D` across 2024-03-10 New York
  keeps the wall clock and elapses 23h; the 2026-03-08 02:30 gap under each of
  the four disambiguation modes; `zdt_until!` largest=day gives `d1 h1`.
- round-trips: `date_to_str` → `date_from_str!`, `date_format`/`date_parse`
  over the whole directive table, `zdt_to_str!` → `zdt_from_str!`.
- the `1970-1-1` assertion becomes `1970-01-01` — the padding bug the current
  gate enshrines.

## Open, and deliberately not decided here

- **Resource-drop coverage.** Dropping `live!` was right for the public
  surface, but the package now has no proof its destructors run; the gauge
  lives only in `tests/golden/b7-temporal`. trantor-cli's `TempTest` is
  precedent for a test-only module shipped in a package. Needs a call before
  release.
- **`b7-temporal` still duplicates the interface and host** byte-for-byte apart
  from `live!`. b8 was converted to consume trantor-cli; b7 was never converted
  to consume this package, so the fixture and the released API can drift.
  Converting it and the gauge question are the same piece of work.
- **`temporal_rs = "=0.2.6"` freezes the bundled tzdb.** What is current
  upstream was not measured; only 0.2.6 is in the cargo cache.

# T1 — trantor-temporal for release: design log (2026-09-12)

**Plan:** [`plans/2026-09-12-t1-temporal-release.md`](../plans/2026-09-12-t1-temporal-release.md).

Settled against trantor `836f635`, trantor-temporal `5c6e000`, temporal_rs
`0.2.6`. The brief: review the package's API before it is released, since a
hosted leaf's signature is the one thing a package cannot change afterwards.

The package left trantor-cli one commit ago carrying b7's fixture surface
unchanged apart from dropping the `live!` gauge. Nothing about that surface had
been chosen for consumers; it was chosen to make a gate pass.

## What was measured before any question was asked

Every number below came from running temporal_rs 0.2.6 directly, not from
reading the spec.

- **`PlainDate` is ISO on the way in and calendar-projected on the way out.**
  `date_of` feeds the record to `PlainDate::try_new`, which interprets the
  fields as ISO and merely attaches the calendar; `rec_of` reads them back
  through the calendar-derived `year()/month()/day()`. So a zero duration is
  not an identity:

  ```
  hebrew    date_add!({2024,1,31}, zero) -> {5784, 5, 21}
  buddhist  date_add!({2024,1,31}, zero) -> {2567, 1, 31}
  iso8601   date_add!({2024,1,31}, zero) -> {2024, 1, 31}
  ```

  The result cannot be fed back in — it would be re-read as ISO year 5784. The
  shipped doc comment says "plain records (ISO fields)", true only of the
  input. `d.with_calendar(Calendar::ISO)` before reading restores the identity
  (`-> {2024, 1, 31}`) while preserving calendar-aware arithmetic: a Hebrew
  +1 month from 2024-01-31 still lands on 2024-03-01 ISO, which is a Hebrew
  month step, not a Gregorian one.

- **Two leaves swallow errors into `""`.** `TimeZone::identifier()` and
  `ZonedDateTime::to_ixdtf_string()` both return `TemporalResult`; the host
  ends both with `.unwrap_or_default()`, and both Roc signatures are
  infallible. An empty string is a plausible-looking answer for a failure.

- **`ErrorKind::Syntax` has zero construction sites in 0.2.6.** `Type` has 20,
  `general` (→ `Generic`) has 4. The Roc `Err` union advertises a `Syntax`
  variant the backing crate cannot produce.

- **Error classification reads the message, not the kind.** `to_err` matches
  substrings against `format!("{e:?}")`, whose Debug form is
  `TemporalError { kind: Range, msg: String("unknown calendar") }` — message
  included. It is correct today only because no `Type` or `Generic` message
  contains "Range"; all 14 `Type` messages were checked. `Display` is
  `"{kind}: {msg}"` (`"RangeError: unknown calendar"`), so the kind is
  available as a reliable prefix — and the host currently puts that whole
  string into the payload, so a consumer sees `Range("RangeError: …")` with the
  kind said twice.

- **`until` defaults differ per type, and the package hardcodes both.**

  ```
  date until 2024-01-01..2024-12-25   default -> d359        largest=year -> m11 d24
  zdt  until across spring-forward    default -> h24         largest=day  -> d1 h1
  ```

  `DifferenceSettings::default()` is all-`None`; temporal_rs resolves it to day
  for dates and hour for zoned, matching TC39. So `date_until!` returns a
  ten-field record in which nine fields are permanently zero, and nothing in
  the shipped `.roc` says so.

- **Zoned arithmetic is not expressible from the rest of the surface.**

  ```
  z0 = 2024-03-09T10:40:00-05:00[America/New_York]
  z0 + P1D = 2024-03-10T10:40:00-04:00[America/New_York]   elapsed 82800000000000 ns (23h)
  ```

  Temporal does date parts in wall-clock and time parts in exact time, so
  decomposing to PlainDate/PlainTime and reconstructing double-applies the
  offset for mixed durations. Adding 86400e9 ns is wrong twice a year.

- **All four disambiguation modes behave in New York.** Across the 2026 gap and
  overlap — and only there, which is the measurement's weakness: D-T1-23 later
  found them wrong in 16 of 51 zones:

  ```
  GAP  02:30   Compatible 03:30-04:00  Earlier 01:30-05:00  Later 03:30-04:00  Reject RangeError
  OVER 01:30   Compatible 01:30-04:00  Earlier 01:30-04:00  Later 01:30-05:00  Reject RangeError
  ```

  `PlainDate::to_zoned_date_time` has no knob and silently picks Compatible.

- **trantor-cli is two-layered; trantor-temporal is not.** `Fs.open_at!` takes
  `flags: U8` ("0 = read, 1 = write") and `File`/`Path`/`Utc` are pure-Roc
  modules over the raw leaves. Both surfaces are exported (D-U1-17). In
  trantor-temporal the hosted interface *is* the public API, so every default
  would have to be hardcoded in Rust or spent as an ABI leaf.

- **The keyword escaping is not ours.** `generated.rs` comes from upstream
  `RustGlue.roc` via `roc glue`; `build.rs:195` only writes the output. `TypeErr`
  exists because `Type` lowercases to a Rust keyword the glue does not escape
  with `r#`.

- **The package's gate was red when this work started, through the baseline.**
  (Since resolved: trantor-cli's net split landed.) `verify.sh` exits 1 at
  step (2) with `platform/Sockets.roc: FileNotFound` — trantor-cli is mid-split
  to trantor-net (uncommitted: `Host.roc` still `import Sockets`, `net-lib` /
  `sockets-host` / `http-host` still present, `TempTest.roc` deleted). Nothing
  in trantor-temporal is at fault. `verify.sh:74` sends the build's stderr to
  `/dev/null`, so the failure reported only "FAIL: build/run with both
  packages" and the cause had to be reproduced by hand.

## Decisions

**D-T1-1 — trantor-temporal becomes two-layered, like trantor-cli.** A hosted
`TemporalHost` that takes every option explicitly and holds no defaults, and a
pure-Roc `Temporal` module that carries the defaults, the doc comments and the
examples. The shim keeps the plain name and the hosted module takes the `*Host`
suffix, as `Random`/`RandomHost` and `Locale`/`LocaleHost` already do, so
`import pf.Temporal` stays true. Both exported (D-U1-17).

This is the decision the rest depend on. Single-layered, every ergonomic
choice is an ABI choice: "8601 by default" would be a hardcode or an extra
leaf, and #7's and #9's defaults could not exist without one of the two.

**D-T1-2 — `PlainDate` and `PlainTime` records are ISO fields on both sides.**
`rec_of` reads through `with_calendar(Calendar::ISO)`. A `Calendar` resource
therefore means exactly one thing — which rules the *arithmetic* follows —
and never a field representation. Measured above: this keeps Hebrew month
stepping while making the record round-trippable.

The alternative, letting the record carry its calendar, would mean either a
fourth field or a fourth resource, and every consumer would have to ask which
calendar a record is in before reading `.year`.

**D-T1-3 — zoned arithmetic goes in: `zdt_add!`, `zdt_until!`,
`zdt_start_of_day!`.** None is expressible from the rest of the surface, per
the measurement above. `subtract`, `since` and `equals` are *not* leaves — a
`Duration` is a plain record so the shim negates it in Roc, `since` is `until`
with the arguments swapped, and `equals` is `epoch_ns` plus the zone and
calendar ids. Three leaves, not six.

`start_of_day` earns its leaf because DST-aware midnight is not 00:00
wall-clock — some Brazilian days begin at 01:00, and constructing 00:00 there
would shift or reject.

**D-T1-4 — an option is an explicit argument on the leaf and a default in the
shim.** Applied three times: `Unit` on `date_until!`/`zdt_until!` (#7),
`Overflow` on `date_add!`/`zdt_add!` (#9), `Disambiguation` on
`zdt_from_wall_clock!`. Each shim pairs a defaulted `until!`/`add!` with an
explicit `until_in!`/`add_with_overflow!`.

The defaults are TC39's, which temporal_rs already resolves to: day for
`date_until!`, hour for `zdt_until!`, `Constrain` for arithmetic, `Compatible`
for disambiguation. A package whose pitch is being Temporal-shaped should not
answer a ported program's question differently with no error. `smallest_unit`,
`rounding_mode` and `increment` stay out of 0.1.0: they change precision, not
the shape of the answer.

Options cross as payload-less tag unions, not `U8` codes — `Fs.stat_at!`
already sends `[File, Dir, SymLink, Other]` across, and no unit, overflow or
disambiguation name lowercases to a Rust keyword.

**D-T1-5 — a malformed record always rejects; `Overflow` governs only the
arithmetic.** `{2024, 2, 30}` is a bad value, not a date that overflowed;
silently making it Feb 29 hides the bug that produced it. `2024-01-31 + 1
month` genuinely has no Feb 31, and constraining there is the meaningful
Temporal answer. This is what the code already did — across
`PlainDate::try_new` (Reject) and `add(…, Constrain)` — it just never said so.
Date text from users goes through `date_from_str!`, which has its own
strictness.

**D-T1-6 — calendars everywhere, not ISO-only zoned.** Both zoned constructors
take a `Calendar` and `zdt_calendar!` reads it back. With D-T1-2 in place the
asymmetry would otherwise be visible and unexplainable: pick a calendar for
date arithmetic, lose it silently at the zoned boundary. One new leaf, one
changed signature, and `zdt_to_str!` emits the IXDTF `[u-ca=…]` annotation
correctly.

Dropping calendars entirely was the sharper alternative — four fewer leaves and
the D-T1-2 bug class becomes structurally impossible — but adding them back
later would change released signatures, which is the phased path this project
does not take.

**D-T1-7 — `Err` uses Roc vocabulary: `[OutOfRange(Str), Invalid(Str),
Other(Str)]`.** `Syntax` goes because the backing crate cannot produce it.
RangeError/TypeError are JS names that tell a Roc consumer nothing, and no name
here lowercases to a Rust keyword — so `TypeErr`'s workaround spelling
disappears structurally rather than being carried into a released API or paid
for by string-patching `roc glue` output in a second repo.

Classification reads `Display`'s kind prefix rather than scanning Debug for
substrings, and the payload carries the message with that prefix stripped, so
the kind is said once.

**D-T1-8 — formatting lives in the shim and is pure; parsing is a leaf.**
Because D-T1-2 makes records genuinely ISO, weekday and ordinal are computable
from `{year, month, day}` in Roc (Sakamoto), so `date_format : PlainDate, Str
-> Str` needs no `!`, no leaf and no new Rust dependency. Parsing IXDTF is not
symmetric with printing it — annotations, calendar resolution and
offset-vs-zone conflict are temporal_rs's job — so `date_from_str!`,
`zdt_from_str!` and `duration_from_str!` are leaves.

`date_to_str` is then literally `date_format(d, "%Y-%m-%d")`, which also fixes
the unpadded `1970-1-1` the current gate asserts.

**D-T1-9 — strftime, not LDML.** `%Y-%m-%d`. Familiar from C, Python, Ruby and
`date(1)`, which is the audience a CLI platform has. LDML would be the
forward-looking choice only if locale-aware formatting arrives, and that was
explicitly not chosen; its case traps (`MM` vs `mm`, `YYYY` vs `yyyy`) are a
known bug source. Literal `%` escapes as `%%`.

**D-T1-10 — pattern parsing mirrors the formatter, strictly.** `date_parse(str,
pattern)`: literals must match exactly, the whole input must be consumed, and
every field the pattern names must be present. `%b`/`%a` are case-insensitive,
because case carries no information in a month or weekday name and fixed-width
reports upper-case them. `%y` maps 00–68 → 2000s and 69–99 → 1900s per POSIX,
documented at the leaf.

Missing fields error rather than defaulting. A year silently defaulting to 1970
is the bug that ships.

**D-T1-11 — no `PlainDateTime`.** Deliberate divergence from TC39, recorded so
it is not re-litigated: a zoned value or a date-and-time pair covers the cases
a CLI platform has, and the third type would double the arithmetic surface.

**D-T1-12 — offset-vs-zone conflicts reject.** When `zdt_from_str!` is given
`…-05:00[America/New_York]` whose offset and zone disagree, that is an error,
matching TC39's default for `ZonedDateTime.from`. A string that carries its own
offset is asserting something checkable; silently preferring one side would
make a wrong string produce a plausible instant.

**D-T1-13 — the upstream wall-clock defect, and that the host must work around
it.** Found while verifying M1, in temporal_rs 0.2.6 — which `cargo search`
confirms is the newest published release, so there is no upgrade to take.
Resolving a wall clock against a zone picks the wrong side of a nearby DST
transition: the instant lands an hour off, and the value disagrees with itself,
because `to_plain_time` reads the stale cached offset while `to_ixdtf_string`
recomputes it. Measured over 2026 at every hour: 24 wrong values for
America/New_York, 213 for Europe/Berlin, 214 for Europe/London, 24 for
Pacific/Auckland, 1 for America/Santiago, none for Sydney or Tokyo. It reproduces through
`PlainDate::to_zoned_date_time` as well as `from_partial`, and with no
disambiguation argument at all, so it is not ours and not the option.

`zdt_from_str!` meets it too: a string naming a zone but no offset resolves a
wall clock, and `2026-03-07T10:40:00[America/New_York]` parsed to 09:40.

Shipping the defect documented, or holding the leaf out of 0.1.0, were the
alternatives. A constructor that silently returns the wrong instant — and hands
back a value whose answer depends on which accessor you call — is the kind of
bug that becomes someone's incident. **How the host works around it is
D-T1-18**, which replaced a first attempt that compared cached offsets and made
correct answers wrong.

**D-T1-14 — the same defect on the arithmetic path, corrected by the
operation's own invariant.** Found while verifying M2: `zdt_subtract!` did not
round-trip. 2026-03-08T10:40-04:00 minus P1D lands on 09:40 rather than 10:40.
Measured at 10:40 across 2026, raw `add` loses the wall clock in BOTH
directions and equally often — New York 1 forward and 1 backward, Berlin 9 and
9, Auckland 1 and 1.

Two things this settled. `subtract(P1D)` and `add(-P1D)` agree *exactly*,
including on the wrong answer — so D-T1-3's claim that subtract is free in the
shim holds, and no leaf is owed. And an offset comparison cannot see this one:
the result is self-consistent, a real instant with the right offset for it,
just not the instant that was asked for.

The correction is the operation's own invariant. A duration with no time parts
moves the date in wall-clock time, so the time of day does not change — that is
precisely what makes `+P1D` across a transition 23 hours instead of 24. When it
does change, keep the date the arithmetic chose, put the original time of day
back, and resolve that wall clock. Mixed durations are untouched, since theirs
is supposed to move. It is direction-agnostic, so it repairs both.

Wall-clock resolution near a DST transition is unreliable in temporal_rs 0.2.6
on the construction path, the arithmetic path and in `start_of_day` (D-T1-19).
Every workaround is conditional on detecting a wrong answer, so each retires
itself when upstream fixes this.

**D-T1-15 — the web comparison drove the surface out, and the shapes follow
from the compiler.** Measured against the current TC39 surface (spec-compliant
polyfill docs, not memory). Where the web passes zones and calendars as
strings, this passes resources — the P5 model's cost, and the difference a
reader meets first. What was genuinely missing came in:

- **`with`** as per-field setters on the records (`date_with_day`) and
  whole-value `zdt_with_plain_date!`/`_plain_time!`/`_calendar!` on the
  resource. The web's `with({ day: 1 })` needs partial records and this
  compiler has none; one function per field says the same thing. The zoned ones
  rebuild through the wall-clock path rather than temporal_rs's `with`, so they
  inherit D-T1-13's correction — changing the date of a zoned value is exactly
  the operation the upstream defect spoils.
- **Rounding, totals and `compare`**, with `RelativeTo` as
  `[Unanchored, ToDate(PlainDate)]`: a duration in calendar units has no fixed
  length until it is anchored, and `Unanchored` is an error for those units
  rather than a guess. One month totals 29 days anchored to February 2024 and
  refuses to answer unanchored, which is the right pair of behaviours.
- **Calendar accessors as ONE leaf** returning a record of all twelve.
  They are calendar-aware, so each would otherwise be its own crossing, and
  twelve crossings for one date is what this package left a baseline to avoid.
- **`hours_in_day` and `getTimeZoneTransition`**, and the rest of TC39's
  difference options as a `DiffOptions` RECORD rather than four more arguments
  — which is also the shape the web's `until(other, options)` takes, and it
  keeps a seven-argument leaf off the ABI.

`compare` returns `[Before, Same, After]`, not the web's -1/0/1: a tag says
which way round it is without anyone recalling the convention.

Not taken: `PlainDateTime` (D-T1-11 stands), `PlainYearMonth`/`PlainMonthDay`,
`Instant` as a type, and locale-aware formatting. The package keeps what the
web has not got, too — strftime formatting and strict pattern parsing.

**D-T1-16 — a third upstream defect, in `start_of_day`, corrected by its own
invariant.** Found by `hours_in_day` returning 25 for an ordinary day. A day
starts at midnight on ITS OWN date, and temporal_rs 0.2.6 breaks that in the
same window as the other two: asked for the start of 2026-03-07 in New York it
answers `2026-03-06T23:00`, an hour early and on the wrong date.

Measured over every day of 2026, walking local dates: upstream finds **two
short and two long days for America/New_York, where there is exactly one of
each, and four of each for Europe/Berlin**. Australia/Sydney was already right
(one and one) and stays right, so the correction does not disturb a correct
answer. Tokyo and São Paulo, which have no transition in 2026, read 365 x 24
either way.

The guard had to read the result through `try_new` rather than its own
accessors: a first attempt compared `to_plain_date()`, which goes through the
very cached offset that is wrong, and so never fired. `hours_in_day` is then
computed from one corrected day-start to the next rather than taken from
upstream, since it inherits the same defect.

That is three defects in the newest published temporal_rs, all in wall-clock
resolution near a DST transition, all corrected conditionally so that each
retires itself when upstream lands a fix. Worth saying plainly in the README,
which it is.

**D-T1-17 — the clock is its own interface, wired by default.** `temporal-now`
(`NowHost`) with its own `now-host` component, and a `Now` Roc module over it.
Reading the wall clock and the machine's configured zone is ambient authority,
and this project routes that through interfaces everywhere else; a world's
composed wiring now names it.

It is wired by default rather than opt-in, and the reason is an integration
fact rather than a preference: a package's `exports` are unioned into the
consuming world unconditionally (`deps.rs:237`) and pure-Roc components' modules
are copied unconditionally (`codegen.rs:168`), while interface BINDING modules
follow `world.wiring` (`codegen.rs:159`). So an unwired `temporal-now` would
export a `Now` module importing a `NowHost` that was never generated — the same
`FileNotFound` the half-finished net split produced. Genuine opt-in needs a
separate PACKAGE; that was offered and not taken, and "named but not removable"
is still a long way from a clock hidden inside a datetime library.

`now-host` vendors no temporal_rs: `std::time::SystemTime` for the instant and
`iana-time-zone` for the zone, which is precisely what temporal_rs's `sys-local`
feature uses. `temporal-host` then drops that default feature, so each native
lands in exactly one archive — measured: temporal_rs 856 symbols in
`libtemporal_host.a` and 0 elsewhere, iana-time-zone 5 in `libnow_host.a` and 0
elsewhere. Before that change iana-time-zone was in both.

`iana-time-zone` asks CoreFoundation for the zone on macOS, which the link
found missing until `frameworks = ["CoreFoundation"]` was declared — the
mechanism already existed.

**Finding, not a decision: glue mis-lays-out `Try(I128, [Tag(Str)])`.** The
first shape tried for `epoch_ns!` was `Try(I128, [ClockUnavailable(Str)])`, and
the generated ABI's OWN size and tag-offset assertions refuse to compile it —
an i128 payload beside a Str-carrying error tag. It fails loudly rather than
corrupting, which is the assertion doing its job. Both leaves now carry
payload-less error tags, matching `Clocks.wall_now!`'s proven shape; both
failures mean a misconfigured machine, which a tag says as well as a sentence.

## Adversarial review (2026-09-12), and what it cost

Three independent reviews, run after the package was declared ready. Every
severe finding was re-verified before being acted on. They found, in the
shipped package: three process aborts reachable from ordinary calls, two
resource leaks, a correction that made correct answers wrong, a documented
function that could not do its job, and silently wrong durations.

**The single cause is one habit, not fifteen bugs.** Every defect lived on an
input that had not been thought to try — and every verification built for this
work was constructed from the cases already in mind. The 49-behaviour gate, the
drop-balance probe and D-T1-13's sweep all came back green because green meant
"consistent with my assumptions", and that was reported as "correct". Two
measurements in this very document were falsified by sampling that excluded the
hour and the direction where the failures lived.

**D-T1-18 — a wall clock is corrected by its invariant, not by a proxy for it.**
Replaces D-T1-13's `corrected`. The question a resolution has to answer is
"does this instant read back, in this zone, as the wall clock that was asked
for?" — so that is now the test. Accept what temporal_rs produced when it
satisfies the invariant; otherwise try the offset-difference shift, which
repairs the real defect; accept THAT only if it satisfies the invariant too. If
neither does, the wall clock does not exist and `Disambiguation` has already
chosen. Every path returns a `try_new` value, so a result can no longer
disagree with itself.

Measured over 23 zones x 3 years x **all 24 hours**, 604,875 unambiguous wall
clocks: the old correction is wrong 37 times, the new one 0. The hour count is
the point — the old sweep's three hours per day is what hid the regression.

The string path gets the same treatment: what a string asked for is what it
says, so it is parsed again as a plain date-time and handed to the same test.

**D-T1-19 — `start_of_day` is searched, not corrected.** Where a zone SKIPS
midnight, the wall clock asked for does not exist, so no candidate can satisfy
D-T1-18's invariant and it rightly declines to choose; asking for the start of
2026-09-06 in Santiago returned the previous day. Midday always exists and 26
hours earlier is always outside the day, so bisect between them for the
boundary where the local date flips. ~47 halvings, exact in every zone.
Verified against an independent linear scan over 16,440 days in 15 zones x 3
years (1988-2026 spot-checked since): 0 mismatches over that range, and every
year's day lengths sum to the year. NOT exact everywhere: where a backward
transition carries local time across midnight into the previous date, the
predicate it bisects is not monotone and it converges on the second boundary —
`Antarctica/Casey 2010-03-05` and `America/St_Johns 1988-10-30` are 3 and 2
hours late. Four zone-dates over every tzdb transition; still open.

**D-T1-20 — a Roc record has no constructor, so the check moves to the use.**
TC39 refuses to construct a mixed-sign `Duration` (`RangeError`, measured); our
`Duration` is a plain record and cannot refuse, so `duration_to_str` returns
`Err(MixedSigns)` rather than inventing a value. It had been inventing:
`{days:1, hours:-2}` printed `P1DT2H`. The same principle explains the pure
formatters, which are total over unvalidated records and must therefore answer
for `{month: 0}` rather than abort: out-of-range values print whole, unknown
names render `?`, and `day_of_week` answers 0 for a record that names no date.

**D-T1-21 — a missing year is the current year, and that makes it an effect.**
`Now.date_parse!` fills it from the clock, because `"%d/%m"` means one date in
December and another in January. `Temporal.date_parse_in` takes the year
explicitly and stays pure, which is what a test can pin. `time_parse` requires
neither: a time has no date, and demanding one is why it could not parse a time
at all.

**The gate now leaves New York.** Every DST number in this document was
measured in Berlin, Sydney or Santiago and none of them was protected, which is
exactly how a correction that broke Berlin passed a green gate. It also
exercises error tags, out-of-range input to every pure function, and a value
checked against itself — and it is verified to FAIL when the old correction is
reinstated.

## The b8 flake, and five hypotheses that were wrong (2026-09-12)

Running the full golden suite as the last release check turned up a failure in
`b8-basic-cli` — the only fixture that consumes two packages. It is recorded
here because the investigation produced exactly one durable thing, and it was
not an explanation.

**What was seen.** Two failures, in the first two suite runs, with two
DIFFERENT symptoms: a roc panic (`hosted extern "trantor__cli_host__args" was
specialized at type c5423ec4dfe627b4 instead of ... 5da93e1a9fafc143`), then a
later step failing with nothing useful on the console. Since then: 5
consecutive suite runs green (115 fixture runs), b8 passing standalone, under
the runner alone, and through 10 clean-tree builds. It has not recurred.

**Every explanation offered was falsified by a direct test.** Kept because the
list is the actual result:

| hypothesis | test | outcome |
|---|---|---|
| A real type mismatch after trantor-cli's refactors | compared glue's type against the host's signature | matched — not a mismatch |
| Stale generated platform | wiped `target/trantor`, rebuilt | reproduced once, then never again |
| trantor-cli itself broken | ran its gate alone | PASSES, 227 expects — the earlier failure was MY concurrent rebuild competing with it |
| The suite runner's environment | `just verify b8-basic-cli` | passes under the runner when it is the only fixture |
| First build after a dependency's interface changes | isolated copies of trantor-cli/net/b8; renamed a hosted leaf's tag; rebuilt with no wipe | **passes first build** — refuted |
| Toolchain nondeterminism | 10 clean builds, hashing `abi/src/generated.rs` and the composed platform | **1 distinct hash each, 0 panics** — the toolchain is deterministic |

**D-T1-22 — the suite runner keeps every fixture's full log.** The one change
worth making. It printed `tail -12` of a failure and discarded the rest, so the
second occurrence taught us nothing: the cause was outside the tail and the
evidence went with the runner's return. Complete output now goes to
`target/verify-logs/` for every fixture, pass or fail; a failure's log is also
copied aside under a timestamp so the next run cannot clobber the one
occurrence of a rare fault; and it is written as the script runs, not captured
into a variable, so it survives a killed or hung run. `target/` is gitignored,
so it cannot dirty the tree the suite checks.

The honest state: two failures that cannot be reproduced and are no longer
explained. Nothing implicates the temporal work — b7 passed in every run as a
package consumer, and the package gate passed throughout. Five falsified
hypotheses on two unreproducible events is the point at which more guessing
costs more than waiting for better evidence, which is what D-T1-22 buys.

**D-T1-23 — resolve a wall clock; do not correct someone else's answer.**
Replaces D-T1-18. Taking temporal_rs's instant and repairing it cannot work at
an overlap: BOTH instants satisfy the invariant, so nothing downstream can tell
which side `Disambiguation` asked for. Measured across every tzdb transition,
that version was wrong on 14,961 overlaps across 174 zones and 17,081 gap
resolutions, and `Reject` returned a value 60 times — while the README promised
it was there "to be told rather than guessed for".

So the candidates are computed here, which is what the spec describes. A wall
clock sits at `naive - offset` for each offset the zone uses nearby; those that
read back are the instants that exist. One: the answer. Two: `Earlier` and
`Compatible` take the first, `Later` the last, `Reject` errors. None — a gap —
the candidates straddle it, `Earlier` takes the instant before and
`Later`/`Compatible` the one after, which is the spec's rule, and `Reject`
errors.

`zdt_add!` implements its operation the same way: date parts in wall-clock
time, then time parts in exact time. Correcting upstream was worse than
useless there — its hour-sized error can cross local midnight, so the repair
read the wrong DATE and rebuilt there, turning one hour of error into
twenty-four. `+P1D` could return the instant it started from (Africa/Cairo,
2024-04-20T00:30), and `hours_in_day` then called that day zero hours long.

Both defects came from trusting a value's own accessors, which read through the
cached offset that is the entire upstream problem. D-T1-19 had already fixed
exactly that in `start_of_day` and the lesson was not carried one function
across.

Measured over 20 zones x 3 years x all 24 hours x 4 disambiguations:
**2,104,320 resolutions, 0 wrong; 1,051,954 add/subtract operations, 0 wrong
dates** — against ground truth computed from real offset lookups.

**D-T1-24 — totality means totality, and a parser must require what it
claims.** `iso_day_of_week` aborted above year ~1.7e9 (I32 overflow) and
`duration_to_str` aborted near I64::MAX (the carry rewrite removed the
multiplication overflow and left the addition) — both in functions whose doc
comments promised otherwise. Dates outside Temporal's own -271821..275760 range
now answer 0, and an unrenderable duration reports `TooLarge`.

`time_parse` had been fixed by DELETING its requirement rather than narrowing
it, so it accepted any pattern at all — `time_parse("hello", "hello")` returned
midnight. It requires a time, as `date_parse_in` requires a date. And a pattern
naming both `%j` and `%m`/`%d` is an error rather than silently discarding one:
`%a` is validated against the date precisely because a line claiming Monday for
a Tuesday is wrong, and this was the same defect one directive over.

`zdt_transition!(Next)` could return the instant it was asked about — past the
tzif data temporal_rs runs the `Previous` branch for `Next` behind a
`debug_assert`, and hosts are built `--release`. A caller walking transitions
forward never terminated. A non-advancing answer is now no further transition.

## D-T1-25 — a day's first instant is looked up, not searched for

`start_of_day` bisected for the midnight boundary between midday and 26 hours
earlier. That assumes the local date is entered exactly once, so the predicate
"is this instant on that date" is false-then-true. Where a backward transition
carries local time back across midnight it is false-true-false-true, and a
bisection settles on whichever boundary its midpoints happen to straddle.

Measured against a ground truth found by scanning every second of the
surrounding sixty hours, over **all 598 zones tzdb ships, 1900-2100**:

| | zone-days |
|---|---|
| checked | 260,453 |
| local date entered twice (two midnights) | 795 |
| local date never entered at midnight (a gap) | 4,210 |
| bisection wrong | **683** |
| spec algorithm wrong | **0** |

The 683 split 4 / 679. The four are the ones review named
(`Antarctica/Casey 2010-03-05`, `America/St_Johns` / `Goose_Bay` /
`Canada/Newfoundland` 1988-10-30). The other 679 are at the two ends of the
representable range, where the unconditional 26-hour probe is unrepresentable
and the error propagated — including UTC's own first day, which begins at the
minimum instant exactly. Asking for it returned `OutOfRange` for an answer that
is not merely representable but is the boundary itself.

What matters more than the four is that the bisection was right on 791 of the
795 double-midnight days **by arithmetic luck** — whether a midpoint lands
inside the sliver of local time before the clocks go back. `America/Goose_Bay`
is wrong in 1988 and right in 1989 for no reason but where the midpoints fell.
Moving one transition by a minute redraws that line, so the four were never a
list of known-bad dates to work around; they were the visible part of an
algorithm that had no claim to be right anywhere.

So it does what TC39 specifies: the earliest instant that reads back as
midnight on the date, and where the zone has no midnight there, the transition
that drops local time into the day. `resolve_wall` already enumerates the
candidates (D-T1-23), so the first branch is a call to it with `Earlier` plus a
read-back to tell "the earliest midnight" from "no midnight at all". Both
branches are exercised in bulk by the sweep above, 0 wrong.

This is a REPLACEMENT, not an addition: the function is the same size it was.
That is the argument for it — the bisection was not smaller, it was searching
for something the data structure can be asked for directly.

The gate gained three behaviours (67 -> 70), pinned to the numbers the
independent Rust scan produced, and they were right on the first run through
the Roc surface. Two of the three fail if the bisection is restored. The third
— the gap case — passes either way, because a gap is exactly the shape a
bisection handles correctly; it pins the new code path rather than catching the
old defect, and saying so is cheaper than discovering it later.

An honest note on the measurement: a first census counted "transitions that
cross local midnight" and reported 97 double-midnight dates and 637 gaps. Both
numbers were wrong — a transition landing exactly ON midnight crosses the date
line without skipping midnight, and two offsets in the window produce two
candidate instants that land on the right DATE without either being midnight.
The numbers above count what actually reads back as midnight, which is the
question. The wrong census would not have changed the fix, but it would have
gone into this file as a fact.

## D-T3 — `trantor test` tests packages

Each package had a hand-rolled `verify.sh` (495, 457 and 489 lines) repeating
the same composition steps. `trantor test <dir>` on a directory with
`package.toml` now runs them, and all three gates are deleted. Plan:
`plans/2026-09-13-t3-package-test.md`.

- **D-T3-1** A package names its test baseline in `[dev-deps]`, never expanded
  for a consumer.
- **D-T3-2** README examples are checked by the tool: every ```roc block builds
  and runs, and a comment opening with a literal value is compared. Prelude
  bindings come from `tests/readme-prelude.roc`.
- **D-T3-3** The expect check is a delta over the dev-deps, failing only when
  the sources hold `expect`s and none ran. The floors (140, 5, 200) are gone:
  each was a number someone had to bump, and the temporal one was already
  stale in its own comment.
- **D-T3-4** All three packages converted.
- **D-T3-5** What a stdout diff cannot express is `tests/<n>/test.sh`, one
  section each, carrying the old section's assertions and comments.

What implementing it found, each fixed rather than worked around:

- **The "add-on must fail alone" rule was wrong.** trantor-net has no driver
  of its own but inherits one through `[deps]`. The rule is now whether a
  driver is in reach through the `[deps]` chain.
- **The negative control needs no app.** It reads the composed platform's
  `exposes` — an app would have to match some driver's `main!` contract, and
  `trantor new`'s scaffold still carries an old one.
- **A world's output directory is its DIRECTORY name**, not `[world] name`.
  The core only worked because its scratch worlds happen to be named `app`.
- **README blocks are sometimes whole programs.** trantor-cli's are; they are
  built and run as written rather than stitched into the fragment app.
- **The README port had its own bug on first run:** a name after `..` (a
  spread) was taken for a field access.

Costs, measured: trantor-temporal 37 s, trantor-cli 64 s, trantor-net 100 s —
net's scripts each build their own world where the old gate reused one.
Printing the sweeps' counts is lost; cargo reports pass or fail.

## D-T3b — the review of `trantor test`

Four independent reviewers, each required to reproduce a finding before
reporting it, found that `trantor test` could PASS packages it had not
tested. Decisions taken on their findings (user, 2026-09-13):

- **D-T3-6 `trantor add <org>/<repo> [<dir>]`, as U1 specified.** It had
  drifted to `add <dir> <org/repo>`, edited world.toml only, and never
  composed. It now adds to the world.toml or package.toml in `<dir>`
  (default `.`), pins in that directory's lock, composes, and restores the
  manifest and lock byte for byte if composition fails. The post-add hook that
  scaffolded an app is gone: it built and wrote apps for `--app` and
  cargo_root layouts where no `app/main.roc` is the intent. `new --from` is the
  one place an app is written.
- **D-T3-7 Expects run with every shipped module exposed.** A separate
  `expects` world exposes all modules a package's components export (renames
  resolved) and its interfaces declare; app suites and README examples keep the
  consumer's exposure. A `.roc` file with an expect that belongs to no shipped
  module fails by name. This replaces the delta-only reasoning, which a package
  with one reachable expect cleared no matter how many were unreachable.
- **D-T3-8 README comments are strict.** A comment that looks like a stated
  value must be "text", a single token, or Ok(...)/Err(...) (compared against
  `Str.inspect`), with prose after ` — `; otherwise the run fails naming the
  line. A clock-dependent line may not state one. A ```text block after a whole
  app is its output, ```roc exit=N its status. The report says what was
  compared and that module-level blocks compile but do not run.
- **D-T3-9 A baseline is everything a package stands on**: dev-deps and its own
  [deps]. **package.toml wins** over a stray world.toml unless --world is given.
- **D-T3-10 Every subprocess is bounded**: its own process group, output to
  files, killed at TRANTOR_TEST_TIMEOUT (900 s), leftovers killed on exit.

Measured along the way: 12 GB of scratch worlds from failed runs; three
trantor-net test peers alive an hour after the commit-time run (a PID recorded
inside `$(...)`); `calendar_from_id!` resolving every id to ISO passing the
temporal behaviours after a cleanup removed the only check. All fixed, each with
a test that fails when the fix is reverted.

Not fixed, by decision or scope:
- A stated plain value on an expression whose `to_str` does not exist (a Try
  without `?`) is a compile error, not a diff. It fails; it does not pass.
- A package's own github dependencies resolve through the package's lock, not
  the consuming world's (`deps::expand_into` recurses with the package root). A
  package that does not commit its lock cannot be composed by a consumer at all:
  the compose fails naming the unpinned dependency.
- The removed expect floors (140, 5, 200) stay removed (D-T3-3); D-T3-7 is what
  now catches unreachable expects.

## D-T3c — the review of the T3b fixes

A second round of four reviewers on the fixes found 45 defects, among them
three new ways to pass untested code: a dev-dependency's module silently
replacing the package's own (so its only, failing, expect never ran), an orphan
expect behind a symlinked component, and README claims that looked like prose
to the classifier (`LT`, `-P1D`, `True`). Decisions (user, 2026-09-13):

- **D-T3-11 `update` and `remove` take `add`'s form and work on packages:**
  `update [<name>] [<dir>]` (`--all [<dir>]`), `remove <name> [<dir>]`; the old
  `<dir> <name>` order is refused naming the new one. All three compose after
  the edit and keep it only if that succeeds. A package's lock could not be
  repaired by any command before.
- **D-T3-12 Edits are journaled.** The old manifest and lock are saved to
  `.trantor-edit/` and each file replaced by rename; an `add` killed mid-compose
  used to leave its edit. The next trantor command in that directory undoes an
  interrupted edit, unless its owner is still alive.
- **D-T3-13 Interrupts reach child process groups.** On SIGINT, SIGTERM or
  SIGHUP (unless the caller ignored it) `trantor test` sends SIGTERM to every
  running group, then SIGKILL after a grace period, and dies of the signal it
  received. A child that calls `setsid` has left the group; stopping it is its
  suite's job.
- **D-T3-14 A platform module has one source, for every world.** Two
  components (or an interface and a component) shipping the same
  `platform/<Module>.roc`, or a component exporting a module it does not have,
  is a compose error naming both. Write order used to decide silently.

Also decided in the fixing, within those: a package with no [dev-deps] and no
driver in reach refuses `add`/`update`/`remove` (it cannot be composed, and
expanding its dependencies let a broken one in); `add` refuses a name already
taken — by a different source, by the same package under another name, or in
the lock by another world variant; a README claim is "text", a number, a Bool,
a tag name, text a value renders as, or `Name(...)`, and README values that
read the machine are found through aliases, `exposing`, module-level functions
and prelude bindings over a fixed module list (`Now`, `Utc`, `Clocks`, `Env`,
`Random`, `Locale`, `Cli`, `Stdin`, `File`, `Fs`, `Cmd`, `Subprocess`) — a
package whose machine-reading module has another name is not covered.

## D-T3d — the review of the T3c fixes

A third round of four reviewers found 43 defects, again including passes on
untested code (`expect(...)` and indented module-level expects, components
under `.vendor/` or `tests/`, a four-backtick fence hiding a roc block, `# ->`
claims, f64-rounded numbers, `Path` reads) and a fixture that passed with its
fix reverted. Decisions (user, 2026-09-13):

- **D-T3-15 Recovery restores only what the edit left untouched.** The journal
  also records the bytes the edit wrote; a file that holds something else is
  kept, the journal stays, and the note says how to discard it. Ownership is an
  flock held for the owner's life instead of a pid.
- **D-T3-16 A bare tag name states a value only inline.** `x.compare(y)   # LT`
  is a claim; `# TODO` on a line of its own is prose.
- **D-T3-17 App output stops at a heading.** A ```text block after a whole app
  is its output when no other block and no Markdown heading lies between.

Also fixed within existing decisions: a GitHub `[dev-deps]` baseline is pinned
by `update` and removable by `remove`; a pin another manifest in the directory
uses is kept by `remove`, and a world edit composes every other world variant
with github deps; `add` of a dependency already present changes nothing and names
`update`; interrupted `trantor test` dies of the signal (exit 130), a nested
run's grace is 3 s shorter per level, ignored signals stay ignored, and the
signal pipe is close-on-exec; a killed `new` is cleaned up by the next `new`.

## D-T3e — the review of the T3d fixes

A fourth round found 25 defects. The worst were data loss in the fixes
themselves: the killed-`new` cleanup deleted files the user had added since,
two concurrent `new`s deleted each other's projects, and a journal sweep could
empty a live edit's journal. Decisions (user, 2026-09-13):

- **D-T3-18 A killed `new` is cleaned up only where provably its own.** The
  marker records the bytes `new` wrote; the next `new` removes a file only if it
  still holds them, a directory only if empty, and `target/` whole; anything
  changed makes it refuse by name. The marker is linked into place already
  locked, so two runs cannot share it.
- **D-T3-19 Only effectful calls read the machine.** A README value is
  machine-dependent when it comes from a `!` call through a listed module (or
  its alias, `exposing` list, a value built from it, or a helper, prelude
  binding or type method making one). Pure calls do not count, and a module the
  package under test exports is not a listed one.

Also fixed: the journal sweep checks the owner's pid before its lock; a journal
without its file list is kept, not discarded; world variants are found under
subdirectories and read even when they do not parse, only those using a pin
the edit changed are composed, into scratch output, for package edits too; an
expect in a type module's body and after a mid-line `\\` string is seen; a
second interrupt kills at once; README tag claims with trailing prose fail, a
Bool below a line is still a claim, headings follow CommonMark (indented `#`
is code, setext counts), whole apps' build errors name README lines, and
project names Roc cannot spell in a header are refused instead of escaped.

## D-T3f — the review of the T3e fixes

A fifth round found 19 defects: a finished edit rolled back later (a stale
`.done-<pid>` with a reused pid kept the journal), `add` reading a half-edited
manifest before recovering, a `.trantor-new` able to delete outside its project,
an interrupt note that could stop forwarding (SIGPIPE, a dead terminal), one
interrupt killing a nested run's children without grace, and a machine-read
rule that leaked both ways. Decision (user, 2026-09-13):

- **D-T3-20 A value is machine-dependent when its statement makes an effectful
  call and touches a listed module** — directly, through an alias or
  `exposing`, or through a name whose definition touches one — or uses a value
  such a statement produced. **Supersedes D-T3-19**, whose exclusion of a
  package's own modules let trantor-temporal state `Now`'s year and switched the
  check nearly off for trantor-cli, and whose tracking of `value.method!` missed
  chains, returned values, lambda arguments and fields.

Also fixed: a discarded journal gets a name no earlier run can have left, the
sweep judges by the owner's lock alone (a pid says nothing across containers),
and `add`/`update`/`remove` recover before reading anything; the `new` marker
records its resolved project directory and refuses a marker written for another,
refuses entries reaching outside the project, reads through its locked handle,
falls back where hard links are unsupported, and is gitignored; variant
discovery skips symlinks and directories with their own `world.toml`; the
interrupt note is written after the groups are being ended and cannot kill or
panic, the runner does not signal a group the forwarding thread is ending, and
no command starts after an interrupt; any type-module shape counts as module
level for expects; a `\\` string's interpolation is code; `---` under a fence
is not a heading; `TODO(...)`-style comments are prose; a comment inside a
chained statement keeps build errors on the right line.

## D-T3g — README examples are not checked (2026-09-13)

- **D-T3-21 `trantor test` does not read README.md.** Supersedes D-T3-2, D-T3-8,
  D-T3-16, D-T3-17 and D-T3-20. Checking README examples grew a Roc statement
  joiner, a claim grammar, a generated app, output markers and a machine-read
  rule, and five review rounds kept finding the rule leaking in both directions
  — its knowledge of which calls read the machine lives in other packages. The
  value did not justify the complexity (user, 2026-09-13). A package that wants
  its README examples validated keeps a suite for them under `tests/`. The
  claim-comment conventions in trantor-temporal's README stay as prose;
  `tests/readme-prelude.roc` is gone. The Roc line scanner survives as
  `roc_scan.rs`, for finding module-level expects.
- **D-T3-22 After a killed `new`, the next one explains and removes nothing.**
  Supersedes D-T3-18. Cleaning up after a kill needed fixing in three review
  rounds — a symlink inside the project let a marker entry delete outside it,
  two reruns raced to delete each other's files, an unnormalised path refused
  its own marker. The marker still records what `new` wrote; a run that fails
  undoes its own files while it is running, and a rerun after a kill refuses,
  listing each file as written, changed since, or gone (user, 2026-09-13).

Also fixed from the sixth review: a runner and the forwarding thread now claim
a group before ending it, so a group gets one SIGTERM whichever ends it — an
interrupt during a deadline kill or leftover cleanup used to reach a nested
`trantor test` as a second interrupt — and a command spawned as the interrupt
arrived ends itself; a nested run's grace halves per level (at every depth
shorter than its parent's); a `.`/`{` split across lines still opens a type
body for expects; journal staging and temp names carry a timestamp as well as
the pid; `update <name>` recovers before reading the manifest; a
`variants/world.toml` and a symlinked variant file are variants, and a world
holds a pin only through a github dependency.

## D-T2g — `ZonedDateTime`, `day_of_week`, calendars and rounding in zoned values (2026-09-13)

- **D-T2-7 `ZonedDateTime := TemporalHost.ZonedDateTime.{ … }`.** Supersedes
  the one-field record `{ h : TemporalHost.ZonedDateTime }`. The record existed
  only because the T2 probe found no syntax to unwrap a nominal over
  `Box(U64)`; `ZonedDateTime.(h)` builds one and `|ZonedDateTime.(h)| h`
  unwraps it on `release-fast-10e922df`. Methods still need a nominal, since an
  alias cannot carry them. The record added a field for no reason, so it went
  (user, 2026-09-13). The gate is unchanged: 387 expects, 73 behaviour lines, 4
  sweeps.

- **D-T2-8 `day_of_week` is the only weekday, and pure.** `day_of_week!` went
  to the host for a calendar-aware answer beside the pure `iso_day_of_week`.
  Measured against temporal_rs 0.2.6 over 19.4M calendar/date pairs (every day
  of ISO years 1–3000, and every day of one year in 997 across the full range),
  and through the Roc path for all 16 calendars on four dates: every calendar
  answered the ISO weekday and none refused a date ISO accepts. The host call
  was a slower copy that could fail, so the leaf went and `iso_day_of_week`
  took the name (user, 2026-09-13). `CalendarFields.day_of_week` stays.
- **D-T2-9 A wall clock is resolved in ISO fields, whatever the calendar.**
  `reads_back_as` compared a candidate's calendar date with the ISO date asked
  for, so on any non-ISO calendar nothing read back and every wall clock was
  treated as a gap: London 2026-10-25 12:00 `Earlier` on Hebrew gave 11:00, the
  01:30 overlap took the later side under `Compatible`, and `Reject` refused
  unambiguous times. `with_plain_time`, `start_of_day` and `zoned_from_str` read
  calendar fields as ISO the same way. Found while measuring whether
  `with_plain_*` differ from rebuilding with `zoned_with!`; the gate built zoned
  values on ISO only. Measured after the fix: `resolve_wall` on all 15 other
  calendars equals ISO on 57.6M zone/day/time/disambiguation cases (at least 10
  differences before), and the 320-case `with_plain_*` probe agrees on the
  instant everywhere — the only difference left is `with_plain_date!` keeping
  the zoned value's calendar where rebuilding takes the date's. The sweep is
  not in the gate (133 s); three behaviour lines are.
- **D-T2-10 No `with_plain_time!`.** With D-T2-9 fixed it resolved the same
  instant as `zoned_with!(z.plain_date!(), t, zone, dis)` in all 320 probe cases
  on every calendar, so it and its host leaf went (user, 2026-09-13).
  `with_plain_date!` stays: it keeps the zoned value's calendar.
- **D-T2-11 `round!` is computed from the swept pieces, not by temporal_rs.**
  Upstream rounds a zoned value through the wall-clock direction it gets wrong
  near transitions: across 20 zones in 2026 it gave the wrong day boundary on
  6,728 of 701,920 quarter-hours and the wrong hour on 6,684
  (2026-03-07T00:00-05:00 to the hour gave 23:00 the day before). The gate had
  pinned one wrong value, 10:37:30 `HalfExpand` to the hour as 10:00, because
  behaviour expectations were recorded from output. `round.rs` follows TC39: a
  day rounds by its real length between two starts of day; a time unit rounds
  the wall clock, keeps the offset where it still reads back, else resolves
  `Compatible`. A gate sweep checks 2,964,240 roundings (10 s) and fails on
  upstream's `round`.
- **D-T2-12 A date's `since_rounded!` is `until` from the same receiver, negated.**
  It swapped the arguments to `until_rounded!`. TC39 defines `since` as `until`
  with the rounding mode negated and the result negated, and months counted back
  from the receiver are not months counted forward from the other date:
  2024-01-01 since 2023-11-17 by years was P1M15D, not P1M14D — 43,598 of
  494,100 probed cases. temporal_rs's own `since` agreed with the definition in
  all of them. Written in Roc over the existing leaf.
- **D-T2-13 Every zoned operation goes through an exact tzdb provider.**
  Supersedes D-T1-23 and D-T1-25 (resolution, zoned `add` and start of day
  reimplemented in `zoned.rs`) and D-T2-11 (`round.rs`). Measuring `until_in!`
  found it wrong on 9,914 of 119,952 pairs per date unit near transitions, and
  tracing it found the cause of every wall-clock defect so far: temporal_rs's
  compiled provider estimates the instants a local date-time can mean
  (`candidate_nanoseconds_for_local_epoch_nanoseconds`), and all wall-clock
  resolution goes through it; instant -> offset is exact. A provider wrapping
  the compiled one with that lookup computed from the offsets, passed to
  upstream's `_with_provider` functions, made upstream agree with the oracle
  everywhere measured (resolution 0/78,744, start of day 0/19,686 where the
  stock provider was wrong 2,959 times, round identical to `round.rs`, `until`
  0/479,808). The approved plan was porting TC39's difference and rounding
  (~400 lines); the provider is ~120 and fixes the paths not yet audited too,
  so the user chose it and chose to migrate everything and delete the
  reimplementations (2026-09-13). The provider's gap search bisects on offsets
  rather than calling `get_time_zone_transition`, which past the tzif data can
  answer the query instant. Sweeps now call upstream through the provider, each
  failing with the stock one; resolution gains 2045, `hours_in_day` gains a
  sweep, `until` gains one. Zoned `since_rounded!` became `until` negated, as
  D-T2-12. Not swept: non-ISO calendars, rounded differences, and zones outside
  the twenty except for a day's start.

- **D-T2-14 Transitions are offset changes, found with upstream's `Previous`.**
  Found by review: upstream's `Next` returns the query instant from a zone's
  last tzif entry (20 zones), so `next_transition!` answered `NoTransition`
  with one to come — pinned wrong in the gate for Bucharest 1996 — and misses
  transitions mid-table (27 over 1850-2100, Indiana/Petersburg 2007 among
  them). Both directions report entries that keep the offset, which TC39's
  definition excludes. `transition.rs` uses `Previous` (0 wrong in 128,552
  queries), skips non-changes and bisects for the next. The sweep oracle had
  walked with the same `Next` and skipped 11,142 zone-days while claiming every
  zone; it scans offsets now, under bounds a gate test checks (no zone changes
  offset twice within 12 hours, none reaches 17 hours of offset). The provider
  probes +-18h every 6h on the same bounds: resolution costs 7-10x stock (it was
  15-58x), a compound operation such as resolve then start of day 4-5x, and a
  transition lookup 2.5-15 µs where upstream's is 16-100 ns.
  Also from the review: `equals!` compares zones by primary identifier, and
  `with_time_zone!` no longer leaks a value on an invalid zone. A boundary sweep
  at every transition edge catches the two provider mutants (read back a second
  off, truncate sub-second wall clocks) that passed every earlier sweep.
- **D-T2-15 A weekday needs a date; difference helpers truncate.**
  `PlainDate.day_of_week` returns `Try(U8, Err)`: after D-T2-8 it answered a
  weekday for February 30 or a day outside Temporal's range, where every host
  call refuses one (D-T1-5). `date_diff` and `zoned_diff` use `Trunc`, TC39's
  default, which their docs claimed while they used `HalfExpand` — January 1 to
  February 20 by months was P2M, now P1M. Both are breaking (user, 2026-09-13).

- **D-T2-16 Every zone is swept, near five eras of transitions; one spec gap is
  counted, not compared.** The dense sweeps sample twenty zones; add, round and
  until are now also swept in all 597 within 30 hours of every transition of
  1942-47, 1970-75, 1995-2000, 2021-26 and 2040-41 (7.8M operations, 0 wrong),
  bringing the sweeps to about 75 s (user accepted, 2026-09-13). Round to a day
  assumes an instant precedes the next date's start (TC39 asserts it); a zone
  with a transition at 00:01 local breaks that — Creston, Phoenix, MST and
  US/Arizona in 1943-44, Newfoundland, Moncton and Goose Bay in 1995-2000 (64
  sampled instants after off-minute sampling); Creston's
  clock read 1944-01-01 for a minute and went back to December 31, so 32 sampled
  instants have no spec answer. `round!` keeps temporal_rs's answer there and
  says so (user, 2026-09-13).
- **D-T2-17 Rounded differences are checked against the spec's text, with one
  step read as intended rather than literally.** The oracle transcribes
  RoundRelativeDuration and its nudge operations from proposal-temporal's spec
  source (downloaded with the user's permission; WebFetch returned paraphrase).
  936,360 rounded differences agree. `ComputeNudgeWindow`'s "If r1 = 0, let
  startEpochNs be originEpochNs" measures from the wrong date whenever a larger
  unit remains in the start duration — -1 month -1 day -11 hours rounded to the
  week would be -P1M1W — so the oracle uses the origin only for a zero start
  duration, as temporal_rs and the spec's reference polyfill do; 8,803 checks in
  `until_rounded` alone, and some in every other rounding sweep, differ under the literal
  reading. Likely a spec erratum worth reporting to TC39; not filed.

- **D-T2-18 `iso_day_of_year` needs a date.** Same defect as D-T2-15's
  `day_of_week`: it answered for February 30. It returns `Try(U16, Err)`
  (user, 2026-09-13; breaking).
- **D-T2-19 The host's multi-step zoned operations live where the sweeps can
  compile them.** Building from a wall clock, moving date or zone, IXDTF parse
  and print moved from lib.rs's extern functions into `zoned_ops.rs`, and
  `host_ops.rs` sweeps them in every zone near five eras (4,649,056 checks:
  parsing by TC39's offset matching with reject, printing and back, moving,
  building on a calendar), failing for four plausible mutants. Behaviour
  expectations were audited independently of the code: none of 80 wrong, and
  one new line covers the cases the audit found weak.
- **D-T2-20 Calendars are swept for how they meet zones, not for their own
  arithmetic.** `calendars.rs` checks seven calendars near transitions: instants
  that must equal ISO, and add, until and rounded until by the spec over the
  oracle's resolution with the calendar's CalendarDateAdd/CalendarDateUntil
  taken from temporal_rs (ICU4X). No independent implementation of those
  calendars is available; that arithmetic knows no zones, which is where every
  defect so far has been. 325,962 checks; the sweep fails if the oracle
  substitutes ISO arithmetic, so it samples cases the calendar changes.

- **D-T2-21 A parsed date must exist.** Supersedes D-T1-5 for parsing only: a
  record literal is still checked when used, but `date_parse_in` (and
  `Now.date_parse!`) answer `BadInput` for a date that does not exist —
  `2026-00-10` had parsed, contradicting D-T1-10's strict parsing and leaving
  `day_of_week` to refuse it later (raised by the behaviour audit; user,
  2026-09-13; breaking).
- **D-T2-22 A parsed time must exist.** `%H`, `%I`, `%M` and `%S` read within
  0-23, 1-12, 0-59 and 0-59, and an hour paired with `%p` must be 1-12:
  `time_parse` had accepted `25:00` and `23:60`, and reduced an hour modulo 12
  with AM/PM, so `13 AM` parsed as 01:00. `BadInput` otherwise (user,
  2026-09-13; breaking).

- **D-T2-23 The remaining unswept paths are swept, and host logic lives where
  the sweeps compile it.** From the coverage review (user, 2026-09-13):
  fixed-offset zones, which temporal_rs resolves without a provider (1,295,973
  checks across the whole range); a leak suite that takes both paths through
  every host call on a zoned value under `TRANTOR_RESOURCE_TRACE` and requires as
  many releases as creations, failing with the `with_time_zone!` leak restored;
  plain-date add, until and rounded until against the spec's ISO arithmetic on
  day numbers of the oracle's own (790,915 checks); and durations rounded,
  totalled and compared from a date or none against the spec (254,592). None
  found a defect. Plain-date and duration logic moved from lib.rs into
  `plain_dates.rs` and `durations.rs`. Each sweep was checked against plausible
  mutants; one mutant (DifferenceISODateTime without its sign adjustment)
  survived until unrounded differences were added to the sampled settings.

- **D-T2-24 A duration string names each unit once.** temporal_rs 0.2.6 accepts
  a repeated designator and keeps the last (`PT2H3H` is three hours); TC39's
  grammar does not, so the host refuses repeats and out-of-order designators
  before parsing. Found by a sweep of the parser against the spec's grammar.
- **D-T2-25 A date outside 0000-9999 prints with a sign and six digits.**
  `PlainDate.to_str` used strftime's `%Y`, so `-10000-01-01` printed a string
  `date_from_str!` refused (357 of 717 sampled dates). Found by a Roc suite
  round-tripping the package's printers through the host's parsers.
- **D-T2-26 `%N` is the nanoseconds within the second.** Formatting passed only
  the sub-microsecond field, so `.123456789` formatted as `000000789` while
  parsing read it correctly (240 of 15,690 round trips).
- **D-T2-27 The last coverage round.** Swept as well: sub-second and `reject`
  zoned arithmetic in every zone (4.07M), both ends of the range in every zone
  (132,978 — the spec's CheckISODaysRange means the first instants of a zone
  behind UTC print a string that parses back only without its offset, and the
  oracle's probes were clamped to the range), the functions no test called,
  the pure Roc logic against arithmetic written in the test (499,713), plain
  dates and durations on non-ISO calendars by invariant (442,425), and rounded
  differences and calendars in every zone. The tzdb data is not compared with
  the system's in the gate, since that would depend on each machine's tzdata.

- **D-T2-28 `PlainTime.to_str` prints the fraction of a second.** It printed
  `%H:%M:%S`, dropping sub-second fields, so a printed time did not read back as
  itself. It now follows TC39's `toString` with default precision: the fraction
  when there is one, trailing zeros trimmed (`09:30:00.5`) (user, 2026-09-14).

- **D-T2-29 A strftime pattern reads each field once, and `%p` is 12-hour.** A
  field read twice kept its last value (`%H %H` on "09 13" was 13:00), `%L` with
  `%N` merged the fraction, and `%p` with no hour read as midnight: all
  `BadPattern` now, as `%j` with `%m` already was. An hour with `%p` stays 1-12,
  so a pattern pairing `%H` with `%p` formats `13 PM` and does not parse it —
  chosen over reading `%H` with an agreeing `%p` (user, 2026-09-14).
- **D-T2-30 The third review round.** Two crashes introduced by D-T2-25/26/28
  (a year of more than six digits, a millisecond of 1000 or more — records are
  not validated) are fixed and pinned. Provider lookup errors now surface
  instead of dropping an offset; out-of-range candidates are kept, since the
  spec lists them and then throws (removing them was tried and broke the range
  edges). `provider_bounds` asserts the 42-hour spacing the gap search needs,
  over 1800-2600 and the range's last years. Sweep fixes: thinned every-zone
  samples now keep the off-the-minute instants (they had sampled whole hours
  alone), the fixed-offset add and transition sweeps no longer skip their
  refusals and nones, the leap-year invariant is real, the upper range limit and
  more leak paths are tested, and printed durations are compared with the spec's
  printer. One question stays open: in NudgeToCalendarUnit, when the window was
  shifted and rounding keeps r1, the spec text and temporal_rs nudge to the
  window's start where the polyfill nudges to its end; an oracle following the
  polyfill passes every sweep too, so no sampled input tells them apart.
- **D-T2-31 Annotations are read by TC39's grammar before temporal_rs parses.**
  ixdtf 0.6.6 checks each annotation character against the next one, so it
  refused valid `[f=ab]` and `[foo=ab-c]` and accepted invalid `[foo=-ab]` and
  `[foo=a--b]`. The host (`annotations.rs`) now refuses a malformed
  `key=value` annotation and drops a well-formed non-critical unknown one, which
  the spec ignores. It leaves the calendar, critical flags and zone order to
  temporal_rs, which a 673,680-string sweep shows it handles as
  ParseISODateTime does. The same round added three more sweeps, none of which
  found a defect:
  - `equals` against ECMA-402 §6.5 primary identifiers. temporal_rs follows
    CLDR-style country rules, not raw IANA links: Oslo is not Berlin, and
    Iceland resolves to Reykjavik. Ten same-country links are checked by hand.
    Pacific/Johnston stays on Honolulu, since no UM primary keeps Hawaii time.
  - Rounded `since` as negated `until` with the mode negated, checked against
    temporal_rs's own `since` and in Roc.
  - Zoned `format!` in LMT-offset zones and at extreme years.

  The link table comes from the system's 2026c tzdata.zi against the pinned
  2025c, and agreed on every link temporal_rs resolves.
- **D-T2-32 `PlainDate.to_str` prints the calendar annotation.** It printed
  ISO fields alone, so a non-ISO date parsed back on `Iso` and compared unequal
  to itself; `ZonedDateTime.to_str!` already annotated (D-T1-6). It now appends
  `[u-ca=id]` for any calendar but `Iso`, as TC39's `calendarName: "auto"`
  does. The identifier table is written in Roc so `to_str` stays pure, and
  `tests/strings` holds it to the host's `calendar_id!` by round-tripping dates
  on all sixteen calendars. `format` still prints ISO fields whatever the
  calendar (D-T1-8).
- **D-T2-33 Calendar fields are checked against ICU4C, not ICU4X.**
  temporal_rs reckons calendars with ICU4X (`icu_calendar` 2.3.0), so
  comparing against the ICU4X in the cargo registry would compare it with
  itself. ICU4C 78.3 is a separate implementation, and it is read through
  Node's Intl into a committed table, so the gate needs neither Node nor ICU4C.
  Every day from 1800 to 2200 agrees on era, era year, month code, day, month
  length and months in year for every arithmetic and tabular calendar. Where
  ICU4C departs from TC39's era and month-code proposal, the proposal decides:
  - Japanese dates before 1873 use Gregorian eras.
  - Chinese is compared only over 1900-2100 (Purple Mountain Observatory
    data) and Dangi over 1900-2050 (KASI); outside those ranges the proposal
    leaves them implementation-defined.

  Inside those ranges ICU4C misplaces thirteen month starts, each a new moon
  within minutes of midnight. On all twelve Chinese cases temporal_rs matches
  the Hong Kong Observatory's published tables. On Dangi 2017 it matches the
  new moon computed with Meeus's algorithm (23:57 KST). A second test holds it
  to those published starts. Two Dangi disagreements after 2050 are closer to
  midnight than predictions of Earth's rotation can resolve, and fall outside
  KASI's range anyway.

## Still open (raised, not decided)

- **b8's intermittent failure is unexplained.** Not reproducible after ~20
  builds and 9 suite runs; five hypotheses falsified (above). If it recurs, the
  full log is now preserved — start there rather than from a new guess.
- **The tzdb is frozen at the `=0.2.6` pin.** `sys-local` bundles the database,
  so DST rules do not move until the pin does. 0.2.6 is the newest published
  release (measured, D-T1-13), so the pin is current rather than stale — but it
  will go stale, and a time-zone library needs an answer for what happens then.
- **The package itself has no proof its resources drop.** b7 now consumes the
  package rather than copying it, and supplies `live!` from its own `gauge`
  interface — which is the right place for it, since a drop gauge is test
  apparatus. But it means the balance is measured by a fixture in another
  repository, and nothing in trantor-temporal's own gate would notice a leak.
- **Glue names result types by first declaration** (B7 finding 2):
  `zdt_with_time_zone` reuses `TemporalZdtFromEpochNsResult` because the two
  are structurally identical. Reordering leaves in `interface.toml` renames
  host types, and nothing in the package warns a future editor.

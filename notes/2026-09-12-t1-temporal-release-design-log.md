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

## Still open (raised, not decided)

- **`start_of_day` bisects a non-monotone predicate** in four zone-dates where
  a backward transition carries local time across midnight (`Antarctica/Casey
  2010-03-05`, `America/St_Johns`/`Goose_Bay`/`Canada/Newfoundland`
  1988-10-30), and errors at the minimum representable instant because it
  probes 26 hours earlier unconditionally. Both found by review, neither fixed.
- **b8's intermittent failure is unexplained.** Not reproducible after ~20
  builds and 9 suite runs; five hypotheses falsified (above). If it recurs, the
  full log is now preserved — start there rather than from a new guess.
- **The tzdb is frozen at the `=0.2.6` pin.** `sys-local` bundles the database,
  so DST rules do not move until the pin does. 0.2.6 is the newest published
  release (measured, D-T1-13), so the pin is current rather than stale — but it
  will go stale, and a time-zone library needs an answer for what happens then.
- **The `b7-temporal` fixture still carries a full copy** of the interface and
  the host, byte-identical apart from `live!`. b8 was converted to consume
  trantor-cli; b7 was never converted to consume this package. Two sources of
  truth for a released API, and the fixture is where the destructor-balance
  gauge lives — the package itself has no proof its resources drop.
- **Glue names result types by first declaration** (B7 finding 2):
  `zdt_with_time_zone` reuses `TemporalZdtFromEpochNsResult` because the two
  are structurally identical. Reordering leaves in `interface.toml` renames
  host types, and nothing in the package warns a future editor.

# S4 — `trantor-random`: design log (2026-09-16)

**Plan:** [`plans/2026-09-16-trantor-random.md`](../plans/2026-09-16-trantor-random.md).
**Roadmap:** D-S1-11 step 4 in
[`2026-09-14-s1-stdlib-roadmap-design-log.md`](2026-09-14-s1-stdlib-roadmap-design-log.md).
**Survey rows:** "PRNG algorithms … seeded from the host" and "UUID (v4
random, v7 time-ordered)" in [`2026-09-14-stdlib-survey.md`](2026-09-14-stdlib-survey.md).

Settled against trantor `b5b9016`, trantor-cli as checked out on 2026-09-16,
roc `10e922df83`, Go 1.26.3. The brief from D-S1-11: a host-seeded PRNG and
UUIDs, as a trantor package.

## What was found before any question was asked

- **trantor-cli already exposes seeds.** `interfaces/random` has
  `seed_u64!`/`seed_u32!` reading `/dev/urandom` (`components/random-host`),
  surfaced as `Random.seed_u64!`/`seed_u32!` in `basic-lib/Random.roc`. The
  host binding is named `RandomHost` so basic-cli's `Random` keeps its name
  (B4 note).
- **`Random` is consumed and its name is load-bearing.**
  - `tests/golden/b8-basic-cli/verify.sh:61` builds basic-cli's `random`
    example (the B8 "runs by changing only the URL" gate).
  - trantor-files `Temp.roc:101` and `Tree.roc:154` name files from one
    `seed_u64!`.
  - rocjust (`rocjust`, `rocjust-main`, `rocjust-weaver`) calls it for
    `choose()` words, `uuid()` and temp directory names. rocjust is built on
    `~/Repositories/roc-basic-cli`, not composed by trantor. seahaven's
    `platform/main.roc` exposes a `Random` of its own.
- **rocjust has a pure UUID v4 formatter.** `just/Uuid.roc`: `v4(high, low)`
  from two OS seeds, formatting only, so it can be gated with fixed inputs.
- **roc-solid hand-writes LCGs** in `tests/integration/im-charts/charts.roc`
  and `examples/grid-finance/main.roc`, deterministic on purpose because gates
  pin their output.
- **No PRNG, shuffle or UUID exists in any trantor package.**
- **Roc's state-threading idiom is a tuple, result first.** Builtin
  `Str.decode : src, fmt -> (Try(Str, err), src)` and the numeric and `List`
  `decode`s have that shape; `Iter.custom` steps with
  `state -> Try((item, state), [NoMore])` and calls the state a seed
  (`src/build/roc/Builtin.roc:2792, 3068`). The `{ value, rest }` record is the
  older `parser_for` path.
- **Builtin integers have no conversion from `U64` that a generic function can
  call** (probe below, D-S4-8).
- **Go's `internal/chacha8rand` is the reference implementation of C2SP
  chacha8rand.** Read from `$GOROOT/src/internal/chacha8rand/chacha8.go`:
  - The state is 4 seed words, a 32-word buffer, and counters. Each refill
    runs the block function for 4 blocks (counter `c` steps by 4) and yields
    32 `U64`s. After 4 refills (16 blocks) the last 4 words of the fourth
    buffer become the new seed and are never output; that buffer yields 28.
  - `MarshalBinary` is 48 bytes: `"chacha8:"`, the count of words used since
    the last reseed as big-endian `U64`, and the 4 seed words little-endian.
  - The C2SP spec page mentions a 33-byte serialized state but defines no
    byte format.
  - `Reseed` draws 4 words and re-initialises from them.
  - Seed `"ABCDEFGHIJKLMNOPQRSTUVWXYZ123456"` yields `0xb773b6063d4616a5`,
    `0x1160af22a66abc3c`, `0x8c2599d9418d287c` first.

## Decisions

### D-S4-1 A pure add-on over trantor-cli; no host half

trantor-random is all Roc and depends on trantor-cli for seeding. (User.)

**Why:** the only entropy the package needs is a seed, and trantor-cli
provides it. D-S1-11 put secure random bytes in the skipped `trantor-crypto`;
a host entropy stream here would bring it back in by another name.

**Rejected:** a host component with `Entropy.bytes!(n)`.

### D-S4-2 UUIDs come from a seeded generator, not per-call entropy

`Uuid.v4(rng) -> (Uuid, rng)` is pure. The only effect is seeding the
generator from the OS. (User.)

**Why:** one source of randomness, deterministic tests, and v7 takes the time
as an argument so the package needs no clock. Collision resistance across
processes rests on the generator's seed entropy, which D-S4-5 sets at 256
bits.

**Rejected:** `Uuid.v4!()` making two `seed_u64!` calls per UUID (rocjust's
current approach); both.

### D-S4-3 Two generators: `Rng` is chacha8rand, `FastRng` is xoshiro256++

`Rng` is the default and is used in every example. `FastRng` is for
simulations and gates. (User.)

**Why:** people use v4 UUIDs as unguessable ids and reset tokens despite RFC
9562's warning. xoshiro256++'s state is recoverable from 4 outputs, so every
later draw is predictable; ChaCha8's output is not. Go made ChaCha8 its
default for the same reason. xoshiro256++ is 64-bit add/xor/rotate and costs
far less. Naming by purpose follows D-S1-10.

**Rejected:** xoshiro256++ only; ChaCha8 only; PCG64 (a 128-bit multiply per
step and as predictable as xoshiro). Algorithm names as module names (as in
D-S1-10).

**Condition:** ChaCha8's speed in pure Roc is unmeasured. The plan's gate
(D-S4-12) stops for a decision if it is pathological.

### D-S4-4 Output is stable within a major version

For a given seed, both generators' streams, every draw method's output and
the saved-state format do not change within a major version. Checked against
reference vectors and pinned by expects over exact outputs. (User.)

**Why:** the same promise as D-S1-6. roc-solid's fixed-seed clouds become a
supported use, and a saved seed or saved state replays. The cost is that
every draw method must be right before 1.0, so each uses a published method
rather than an invented one.

**Rejected:** no promise (Rust `rand`); stable forever.

### D-S4-5 Construction: C2SP chacha8rand as Go implements it; 256-bit seeds

`Rng` follows C2SP chacha8rand, checked against Go's `math/rand/v2`
`ChaCha8`. Both generators are seeded with 256 bits: 4 `Random.seed_u64!`
calls. (User.)

**Why:** an external spec plus a second implementation is what D-S4-4's
promise should rest on, and the same seed gives Go's sequence. Four reads of
`/dev/urandom` once per generator cost nothing, and stopping at 128 bits would
be a caveat bought for no saving.

**Rejected:** rand_chacha `ChaCha8Rng` (its only reference is the crate); 128
bits of seed.

**Corrected after the session:** the session described the generator as
carrying a 992-byte buffer (124 words). Go's source shows the equivalent
layout used in practice: a 32-word buffer refilled 4 blocks at a time, with
the reseed words taken from the fourth refill. The output is identical; the
Roc generator follows Go's layout.

### D-S4-6 Draws return `(value, rng)`

Every draw returns a tuple, result first, generator second. A draw that can
fail returns `(Try(..), rng)` with the generator still returned. A generator
is anything with `next_u64 : r -> (U64, r)`. (User; the idiom was pointed out
by the user and found in the builtins.)

**Why:** it is Roc's own shape (`decode`, `Iter.custom`), and a generator can
be passed straight to `Iter.custom` as its state.

**Rejected:** a `{ value, rng }` record (the older `parser_for` shape); the
generator as an infinite `Iter` (its state is lost when iteration stops).

### D-S4-7 Modules: `Rng`, `FastRng`, `RngDraw`, `Uuid`, `UuidV7`, `Weights`

`RngDraw` holds every draw, generic over `r` with `next_u64`. `Rng` and
`FastRng` each forward every draw as a method, so `rng.below(6)` works on
both; user generators call `RngDraw.below(r, 6)`. No module is named
`Random`. (User; the user chose `RngDraw` over `Draw`.)

**Why:** method-call syntax (D-S1-1's reason for keeping methods on types).
`Random` stays trantor-cli's: the B8 gate, trantor-files and rocjust use it,
and every consumer of this package also depends on trantor-cli.

**Rejected:** draws only through the generic module; algorithm names.

### D-S4-8 Scalar draws are named per type

```roc
u64 : r -> (U64, r)
u32 : r -> (U32, r)                 # high 32 bits
u8 : r -> (U8, r)                   # high 8 bits
below : r, U64 -> (U64, r)          # [0, n), Lemire; n == 0 crashes
between_u64 : r, U64, U64 -> (U64, r)   # [lo, hi], bounds swapped if reversed
between_i64 : r, I64, I64 -> (I64, r)   # same, offset through U64
f64 : r -> (F64, r)                 # [0, 1), top 53 bits × 2^-53
between_f64 : r, F64, F64 -> (F64, r)   # [lo, hi), see below
bool : r -> (Bool, r)               # top bit
chance : r, F64 -> (Bool, r)        # f64 < p
bytes : r, U64 -> (List(U8), r)     # whole words LE, tail truncated
```

Other integer types go through `between_u64`/`between_i64` and a builtin
`to_*_wrap`. (User.)

*Settled after the review (user):* `between_f64` is `lo + f64 × (hi − lo)`
over `[lo, hi)`. A result that rounds to `hi` is drawn again, as Rust `rand`
does. Reversed bounds are swapped. A non-finite span crashes with a message.

**Why:** high bits because xoshiro's low bits are its weakest. Inclusive
`between` reaches the full range; `below` covers the exclusive case. A generic
`between : r, n, n -> (n, r)` was probed and cannot be written cleanly:

- Dispatch on a type variable (`N : n`, `N.method(..)`) typechecks and
  resolves per call site.
- No integer type has `from_u64_wrap`. Conversions are methods on the source
  type (`U64.to_i32_wrap`), so the target's name varies. The only uniform
  constructor is `from_le_bytes`, which `U8` and `I8` lack, and it would
  allocate a list and return an impossible `Try` per draw.
- Widening is not uniform either: signed types have `to_u64_wrap`, unsigned
  mostly `to_u64`.
- Found in passing: a call to a method that does not exist (`U64.to_le_bytes`)
  inside a generic body produced no compile error, only "runtime error" when
  the expect ran. Recorded in the upstream gaps note.

**Rejected:** generic over the numeric type (above); exclusive `until(lo, hi)`;
more per-type methods (`between_i32`, …).

### D-S4-9 List draws and `Weights`

```roc
shuffle : r, List(a) -> (List(a), r)
choose : r, List(a) -> (Try(a, [ListWasEmpty]), r)
sample : r, List(a), U64 -> (List(a), r)
pick : r, Weights(a) -> (a, r)

Weights(a) :: ...
from_list : List((a, U64)) -> Try(Weights(a), [ListWasEmpty, AllZero, TotalOverflow])
```

- `shuffle` is Durstenfeld Fisher–Yates: `i` from `len−1` down to 1,
  `j = below(i+1)`, swap.
- `choose` is `below(len)`.
- `sample` is without replacement: the first `k` steps of forward
  Fisher–Yates (`i` from 0, `j` in `[i, len)`), returning those `k` items in
  draw order. `k ≥ len` gives the whole list shuffled by the same walk.
- `Weights` holds a cumulative table. `pick` draws `below(total)` and
  binary-searches it.

(User.)

**Why:** integer weights are exact, so the stable sequence does not rest on
float summation. Validating once at build time means `pick` cannot fail and
costs O(log n). `sample` with replacement is `choose` in a loop.

**Rejected:** `F64` weights; a one-shot `weighted(r, list)` returning `Try`;
Vose's alias method (float or rational setup for O(1) picks).

### D-S4-10 Seeding, fork and saved state

| | `Rng` | `FastRng` |
|---|---|---|
| OS | `from_os! : () => Try(Rng, [RandomErr(IOErr), ..])`, 4 `seed_u64!` as the 4 seed words | same, as the 4 state words |
| exact | `from_words : { w0 : U64, w1 : U64, w2 : U64, w3 : U64 } -> Rng` (the 32 seed bytes read as LE words, as Go's `Init`) | `from_words : {..} -> Try(FastRng, [AllZero])` |
| test seed | `from_u64 : U64 -> Rng`, SplitMix64 expanded to 4 words | same |
| fork | `fork : Rng -> (Rng, Rng)`, returns `(child, parent)`; the child is seeded by 4 draws and the parent continues its stream | `fork` returns `(child, parent)`; the child is a copy advanced by `jump` (2^128 steps), the parent is unchanged |
| saved state | `to_bytes`/`from_bytes` | 32 bytes, the 4 state words LE |

No `Iter` view (`Iter.custom(rng, Unknown, |r| Ok(r.u64()))` is one line), no
mid-stream OS reseed. Saved state is covered by D-S4-4. (User.)

*Settled after the review (user):* `fork` returns `(child, parent)`, result
first per D-S4-6, and on both generators the parent keeps its own stream. The
session's table had `FastRng`'s child replaying the original stream instead.

**Why:** each fork uses the method correct for its algorithm: ChaCha8's
output is unpredictable, so drawn words are an independent seed; xoshiro's
jump gives non-overlapping streams. SplitMix64 expansion is what the xoshiro
authors recommend.

**Corrected after the session:** the session agreed on "C2SP's 33-byte
format, the same as Go's `MarshalBinary`". Neither holds: C2SP defines no byte
format, and Go's is 48 bytes. Raised with the user, who chose to match Go but
not document it publicly. (User.)

- `Rng.to_bytes` writes exactly Go's `MarshalBinary` layout: `"chacha8:"`, the
  words used since the last reseed as big-endian `U64`, the 4 seed words
  little-endian. `from_bytes` accepts exactly what Go's internal
  `chacha8rand.Unmarshal` accepts (48 bytes, used count ≤ 124), not the
  `readbuf:` form `math/rand/v2` adds after a partial `Read`.
- Public docs (README, module and function doc comments) describe the state
  as opaque bytes that load in any later 1.x. They do not mention Go or the
  layout. The layout and the Go check live in the vectors crate, this log and
  the plan.

**Why:** Go gives the format a second implementation to test against, and
format stability (D-S4-4) holds either way. Not advertising it keeps Go
interoperability out of the public promise.

**Rejected:** a trantor-defined 33-byte format (one position byte plus the
32-byte seed), checkable only against itself.

### D-S4-11 `Uuid` and `UuidV7`

- `Uuid :: { hi : U64, lo : U64 }` with equality, hashing and ordering.
  `hi`-then-`lo` is RFC byte order, so v7 sorts by time. *Corrected after the
  session:* nominal types derive none of these (review probe), so they are
  written by hand, and `encoder_for` is format-generic so `Hash.of` works.
- `nil`, `max`, `version : Uuid -> U8`.
- `to_str`: lowercase `8-4-4-4-12`. `from_str`: that form only, either case.
- `to_bytes`/`from_bytes`: 16 bytes big-endian (RFC 9562).
- `encoder_for`/`parser_for` through the string form.
- `v4 : r -> (Uuid, r)`: 2 draws, version and variant bits set.
- `v7 : r, U64 -> (Uuid, r)`: 48-bit `unix_ms`, version, 12-bit `rand_a`,
  variant, 62-bit `rand_b`. Order within a millisecond is random.
- `UuidV7(r)` holds `{ rng, last_ms, counter }`; `new` starts with no last
  millisecond, so the first `next` always seeds the counter at random
  (settled after the review, user).
  `next : UuidV7(r), U64 -> (Uuid, UuidV7(r))` uses RFC 9562 method 1: `rand_a`
  is a 12-bit counter seeded randomly each new millisecond; a clock that goes
  backwards keeps `last_ms`; counter overflow advances `last_ms` by one, under
  RFC 9562 §6.2's allowance for altering the timestamp (method 1 itself says
  to freeze; a pure `next` cannot wait, settled after the review, user). A
  `unix_ms` at or past 2^48 crashes.
  Output is strictly increasing within a process.

`v4`, `v7` and `UuidV7` are generic over `r`; the docs point at `Rng`. The
caller supplies `unix_ms`, e.g. from trantor-cli's `Utc.now!()`. (User.)

**Why:** database keys are the main reason to pick v7, and out-of-order
inserts within a millisecond bloat indexes. The pure `v7` stays for one-offs.
A strict parser is simpler to get exactly right; braces and `urn:uuid:` are
for the caller.

**Rejected:** pure `v7` only; `UuidV7` only; `Uuid` as a `Str`.

### D-S4-12 Verification and the performance gate

- `tests/vectors` generates `RngVectors.roc`, and fails `cargo test` (run by
  `trantor test .`) on drift, as trantor-hash does. References:
  - chacha8rand stream, saved state, fork: Go `math/rand/v2` via a `go run`
    program the crate invokes, plus the C2SP sample output checked in.
  - xoshiro256++, `jump`, SplitMix64: `rand_xoshiro`, pinned with `=`.
  - Every draw method: an independent Rust model written from the documented
    algorithm, over fixed seeds and edge cases.
  - `Uuid`: RFC 9562 Appendix A layouts; the `uuid` crate for string round
    trips and rejections; `UuidV7` edge cases by expects.
- Performance: on a unique generator, 1M `u64` draws allocate nothing after
  construction (both generators); ns per `u64` recorded for both. **Stop and
  ask** if a refill allocates or `Rng` is more than 10× slower than `FastRng`.

(User.)

**Why:** ChaCha8 costs roughly 2–3× xoshiro in Go-style code. 10× in Roc
would mean the buffer is being copied, which is a design problem, not a
tuning detail.

### D-S4-13 No consumer migrates in this step

trantor-random ships with its README and tests. trantor-cli's `Random` is
unchanged except for one doc line pointing to trantor-random. (User.)

- rocjust moves when it moves to trantor; `Uuid.v4(high, low)` produces the
  same format, so it is a direct swap.
- trantor-files keeps `Random.seed_u64!`: a temp name needs one OS draw.
- roc-solid's LCGs stay: gates pin their output.

**Rejected:** vendoring into rocjust the D-S1-8 way. That pattern retired
compiler patches; nothing needs retiring here.

## Still open

Nothing.

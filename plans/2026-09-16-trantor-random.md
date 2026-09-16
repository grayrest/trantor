# S1 step 4 — `trantor-random`

**Design log:** `notes/2026-09-16-s4-trantor-random-design-log.md`, decisions
D-S4-1 to D-S4-13. Repos: `~/dev/roc/trantor-random` (new), `trantor` (docs),
`~/dev/roc/trantor-cli` (one doc line). Compiler: roc `10e922df83`. Go 1.26.3
(reference only).

## Why

trantor apps can get a seed from `Random.seed_u64!` and nothing more. There is
no PRNG, no shuffle or sample, and no UUID in any trantor package; rocjust
and roc-solid each hand-roll what they need. This package adds seeded
generators, the draws on top of them, and UUIDs, in pure Roc (D-S4-1).

## Surface

```roc
## Generators. Both provide next_u64 and forward every RngDraw function.
Rng :: ...        # C2SP chacha8rand
    from_os! : () => Try(Rng, [RandomErr(IOErr), ..])
    from_words : { w0 : U64, w1 : U64, w2 : U64, w3 : U64 } -> Rng
    from_u64 : U64 -> Rng                      # SplitMix64 -> from_words
    next_u64 : Rng -> (U64, Rng)
    fork : Rng -> (Rng, Rng)                   # (child, parent); child from 4 draws, parent continues
    to_bytes : Rng -> List(U8)                 # Go's MarshalBinary layout, 48 bytes (D-S4-10)
    from_bytes : List(U8) -> Try(Rng, [InvalidState])

FastRng :: ...    # xoshiro256++
    from_os! : () => Try(FastRng, [RandomErr(IOErr), ..])
    from_words : { w0 : U64, w1 : U64, w2 : U64, w3 : U64 } -> Try(FastRng, [AllZero])
    from_u64 : U64 -> FastRng
    next_u64 : FastRng -> (U64, FastRng)
    fork : FastRng -> (FastRng, FastRng)       # (child = copy, parent jumped 2^128); D-S4-10 as changed after review
    to_bytes : FastRng -> List(U8)             # 32 bytes, state words LE
    from_bytes : List(U8) -> Try(FastRng, [InvalidState])

## Every function here has `where [r.next_u64 : r -> (U64, r)]`.
RngDraw :: [].{
    u64 : r -> (U64, r)
    u32 : r -> (U32, r)
    u8 : r -> (U8, r)
    below : r, U64 -> (U64, r)
    between_u64 : r, U64, U64 -> (U64, r)
    between_i64 : r, I64, I64 -> (I64, r)
    f64 : r -> (F64, r)
    between_f64 : r, F64, F64 -> (F64, r)
    bool : r -> (Bool, r)
    chance : r, F64 -> (Bool, r)
    bytes : r, U64 -> (List(U8), r)
    shuffle : r, List(a) -> (List(a), r)
    choose : r, List(a) -> (Try(a, [ListWasEmpty]), r)
    sample : r, List(a), U64 -> (List(a), r)
    pick : r, Weights(a) -> (a, r)
}

Weights(a) :: ...
    from_list : List((a, U64)) -> Try(Weights(a), [ListWasEmpty, AllZero, TotalOverflow])
    total : Weights(a) -> U64
    index_for : Weights(a), U64 -> a           # first entry whose cumulative weight > x

Uuid :: { hi : U64, lo : U64 }
    is_eq, is_lt (and the other comparisons), to_hash   # hand-written: nominal types derive none
    nil : Uuid
    max : Uuid
    version : Uuid -> U8
    to_str : Uuid -> Str
    from_str : Str -> Try(Uuid, [InvalidUuid])
    to_bytes : Uuid -> List(U8)
    from_bytes : List(U8) -> Try(Uuid, [InvalidUuid])
    v4 : r -> (Uuid, r)
    v7 : r, U64 -> (Uuid, r)
    encoder_for / parser_for through to_str / from_str, generic over the format

UuidV7(r) :: { rng : r, last_ms : [Unset, At(U64)], counter : U64 }
    new : r -> UuidV7(r)
    next : UuidV7(r), U64 -> (Uuid, UuidV7(r))
    rng : UuidV7(r) -> r
```

`Uuid` needs its methods written by hand (review probe: `==`, `<` and
`Dict.insert` all fail on `Uuid :: { hi : U64, lo : U64 }` without them).
Comparison is `hi` then `lo`. `to_hash` feeds `hi` then `lo`. `encoder_for`
must be format-generic so `Hash.of(uuid)` (trantor-hash) works: a nominal
whose encoder names one format cannot serve `HashFormat` (trantor-hash plan,
probe 3).

`RngDraw.pick` cannot see `Weights`' table, so `Weights` owns the search
(`index_for`); `pick` is `below(total)` then `index_for`.

`(v, $rng) = $rng.next_u64()` inside a `for` loop compiles and runs (review
probe), so README examples may use it.

[ASSUMPTION: `Weights.total`, `Weights.index_for`, `UuidV7.new` and `UuidV7.rng` are not named in
the design log. They are the minimum to build and unwrap the types; drop any
that the implementation shows are unnecessary.]
[ASSUMPTION: `from_os!`'s error is trantor-cli's `RandomErr(IOErr)` passed
through unchanged, as trantor-files `Temp.roc:101` does.]

## Algorithms (exact; D-S4-4 pins every one)

**SplitMix64** (`from_u64`): `z = (s += 0x9e3779b97f4a7c15)`;
`z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9`; `z = (z ^ (z >> 27)) * 0x94d049bb133111eb`;
`z ^ (z >> 31)`. Four outputs are `w0..w3`, all wrapping.

**xoshiro256++**: `result = rotl(s0 + s3, 23) + s0`; `t = s1 << 17`;
`s2 ^= s0; s3 ^= s1; s1 ^= s2; s0 ^= s3; s2 ^= t; s3 = rotl(s3, 45)`.
`jump` uses the published constants `0x180ec6d33cfd0aba, 0xd5a61266f0c9392c,
0xa9582618e03fc9aa, 0x39abdc4529b1661c`, as the reference does: for each
constant, for each bit LSB first, if the bit is set XOR the current state into
an accumulator; step the generator after every bit; the accumulator becomes
the state (`rand_xoshiro-0.8.1/src/xoshiro256plusplus.rs`). `rotl` is written with
`shl_wrap`/`shr_zf_wrap` (builtins have no rotation).

**chacha8rand**, following `$GOROOT/src/internal/chacha8rand` (`chacha8.go`,
`chacha8_generic.go`):
- State: `seed : 4 U64`, `buf : 32 U64` (a `List(U64)`), `i`, `n`, `c`.
- `from_words`: `seed = w`, `buf = block(seed, 0)`, `c = 0`, `i = 0`, `n = 32`.
- `next_u64`: if `i < n`, return `buf[i]` and `i + 1`; otherwise refill.
- Refill: `c += 4`; if `c == 16`, `seed = buf[28..32]`, `c = 0`.
  `buf = block(seed, c)`; `i = 0`; `n = 28` if `c == 12`, else 32.
- `block(seed, c)`: four ChaCha8 blocks with counters `c..c+3`, interleaved as
  `chacha8_generic.go` does: the constant and counter additions are skipped,
  the key is added back (`chacha8_generic.go:192-207`). Go writes 32-bit words
  through an unsafe view of `[32]uint64` as `[16][4]uint32`; in Roc, for
  output row `r` (0..15) and the four lanes `b[r][0..3]`:
  `buf[2r] = b[r][0] | b[r][1] << 32`, `buf[2r+1] = b[r][2] | b[r][3] << 32`.
  Cite the Go lines in comments.
- `buf` must be updated in place (step 2 gate).

**Draws:**
- `u32`: `u64 >> 32`. `u8`: `u64 >> 56`. `bool`: `u64 >> 63 == 1`.
- `below(n)`: Lemire's nearly-divisionless method. `m = x * n` (128-bit);
  `lo = low 64`; if `lo < n`, `t = (0 - n) % n` (wrapping), and while
  `lo < t` draw again. Return `high 64`. `n == 0` crashes with a message.
- `between_u64(lo, hi)`: swap if `lo > hi`; `span = hi - lo` (wrapping); if
  `span == U64.highest`, return `u64`; else `lo + below(span + 1)`.
- `between_i64(lo, hi)`: swap if needed; map through `to_u64_wrap` with the
  sign bit flipped, `between_u64`, map back.
- `f64`: `(u64 >> 11).to_f64() * 2^-53`.
- `between_f64(lo, hi)`: swap if `lo > hi`; `span = hi - lo`; if `span` is not
  finite, crash with a message; loop `x = lo + f64 * span` until `x < hi`
  (one draw per attempt). `lo == hi` returns `lo` without drawing.
  [ASSUMPTION: `lo == hi` is not in the log; `[lo, lo)` is empty, and returning
  `lo` avoids an infinite loop.]
- `chance(p)`: `f64 < p`.
- `bytes(n)`: `ceil(n / 8)` draws, each LE, truncated to `n`.
- `shuffle`: for `i` from `len−1` down to 1, `j = below(i+1)`, swap. `len ≤ 1`
  draws nothing.
- `choose`: empty draws nothing; otherwise one `below(len)`.
- `sample(k)`: `k' = min(k, len)`; for `i` in `0..k'`, `j = i + below(len − i)`,
  swap `i` and `j`, including the last step where `below(1)` still consumes a
  word; return the first `k'` items. `k ≥ len` is the full walk, so it equals a
  forward shuffle, which differs from `shuffle`'s order (documented).
- `pick`: `x = below(total)`, then `Weights.index_for(w, x)`: binary search for
  the first entry whose cumulative weight is `> x`. Zero-weight entries are
  never returned.

**UUID** (RFC 9562):
- v4: `hi = u64`, `lo = u64`; `hi = (hi & ~0xF000) | 0x4000`;
  `lo = (lo & 0x3FFFFFFFFFFFFFFF) | 0x8000000000000000`.
- v7: `hi = (unix_ms & 0xFFFFFFFFFFFF) << 16 | 0x7000 | rand_a (12 bits)`;
  `lo = variant | rand_b (62 bits)`. The first draw gives `rand_a` (its top 12
  bits), the second `rand_b` (its top 62 bits).
- `UuidV7.next(g, now)`: crash with a message if `now ≥ 2^48`.
  - if `last_ms` is `Unset` or `now > last_ms`, reseed `counter` from `u64 >> 53` (11 bits), which
    leaves half the 12-bit space as headroom (RFC 9562 §6.2 method 1
    guidance), and set `last_ms = now`;
  - otherwise `counter += 1`; if `counter` reaches `0x1000`, `last_ms += 1`
    (RFC 9562 §6.2 timestamp allowance) and reseed `counter` as above; if that
    makes `last_ms` reach 2^48, crash;
  - `rand_b` is always fresh, drawn after any counter reseed.
  [ASSUMPTION: the 11-bit seed is the implementation reading of "seeded
  randomly each new millisecond"; state it in the README.]

## Package layout

```
trantor-random/
  package.toml       # add-on; [deps] trantor-cli; exports Rng, FastRng, RngDraw, Weights, Uuid, UuidV7
  README.md          # which generator, the stability promise, examples
  components/random/
    Rng.roc
    ChaCha8.roc      # block function; component export only
    FastRng.roc
    SplitMix64.roc   # component export only
    RngDraw.roc
    Weights.roc
    Uuid.roc
    UuidV7.roc
    RngVectors.roc   # generated; component export only
  tests/
    vectors/         # Cargo.toml + go/ reference program; writes RngVectors.roc
    readme/          # compiles the README examples (trantor-hash tests/readme)
    alloc/           # step 2 gate
```

`package.toml` follows trantor-files' comment style.

## Work — commit at each

1. **Vectors crate.** `tests/vectors` with pinned `rand_xoshiro` and `uuid`,
   and a `go/` program using `math/rand/v2` (`NewChaCha8`, `Uint64`,
   `MarshalBinary`, `UnmarshalBinary`; never `Read`, whose byte buffering does
   not match `bytes(n)` and which switches `MarshalBinary` to a `readbuf:`
   form). Go's output is committed as `go/golden.txt`; `cargo run -- --regen-go`
   calls `go run`, and `cargo test` reads the committed file, so a checkout
   without Go can still check drift. The C2SP sample output (2976 bytes, 372
   words) is checked in verbatim and asserted equal to Go's before any vector
   is written. The crate emits `RngVectors.roc`; `cargo test` fails if the
   committed file differs.
   **Frozen set:** `components/random/RngStable.roc` holds hand-copied expects
   (a few words per generator, one output per draw method, one v4 and v7
   UUID, one saved state) that no tool rewrites. Changing the model and the
   Roc code together still breaks it, which is what enforces D-S4-4.
2. **`ChaCha8.roc`, `Rng.roc`** (`from_words`, `from_u64`, `next_u64`, `fork`).
   Expects against the Go stream for the C2SP seed and 3 random seeds, across at
   least 2 full reseed cycles (≥ 250 words), and against `fork` children. The
   Go program builds the fork reference with 4 `Uint64()` calls whose LE bytes
   seed `NewChaCha8`, then continues the parent (Go has no public fork).
   **Gate:** trantor-cli has no allocation counter, so this runs outside
   `trantor test`: a scratch app on the roc repo's `test/alloc-count` platform
   (`Host.alloc_count!`) with the generator modules copied in, built
   `--opt=speed`. Count allocations after construction for 1M draws each of
   `Rng.next_u64`, `FastRng.next_u64`, `RngDraw.below` on each, and a
   generator held inside `UuidV7`. Time ns per `u64` for both generators.
   Record the numbers and the command here; `tests/alloc` keeps the app. **Stop and ask** if a refill allocates or `Rng` is more than
   10× slower than `FastRng` (D-S4-12).
3. **`SplitMix64.roc`, `FastRng.roc`** with expects against `rand_xoshiro`
   `=0.8.1`: `Xoshiro256PlusPlus::from_seed` (32 LE bytes) and `state()`,
   `seed_from_u64` for SplitMix64 (the review confirmed it matches the spec
   above for 0, 1, 42 and `u64::MAX`), and `jump`. `from_seed` maps an all-zero
   seed to `seed_from_u64(0)`, so `AllZero` is tested in Roc only. This step can run before step 2.
4. **`to_bytes`/`from_bytes`** for both generators. `Rng` round trip at
   used counts 0, 1, 27, 28, 31, 32, 96, 123, 124 and after a reseed; at each,
   our bytes equal Go's `MarshalBinary`, and Go's `UnmarshalBinary` of our
   bytes continues with the same words. `from_bytes` accepts exactly what
   `internal/chacha8rand.Unmarshal` accepts (`chacha8.go:151-169`: 48 bytes,
   `chacha8:` prefix, used count ≤ 124) and rejects count 125, other lengths,
   other prefixes and Go's `readbuf:` form. `FastRng.from_bytes` of an
   all-zero state returns `InvalidState`. Doc comments call
   the state opaque and stable within 1.x; they do not mention Go or the
   layout (D-S4-10).
5. **`RngDraw.roc`, `Weights.roc`**, forwarding methods on `Rng` and
   `FastRng`. The Rust model in `tests/vectors` implements every draw from the
   Algorithms section, not from the Roc code. Vectors run through the
   forwarding methods on both `Rng` and `FastRng` as well as through
   `RngDraw`. Fixed seeds with edge cases: `below(1)`, `below(2^63)`,
   `below(U64.highest)`, `between_u64(0, U64.highest)`,
   `between_i64(I64.lowest, I64.highest)`, reversed
   bounds, `bytes(0)`, `bytes(7)`, `shuffle([])`, `shuffle([x])`,
   `sample(k = 0, len, len + 3)`, `between_f64(1.0, 3.0)` over a seed whose `f64` is `1 − 2^-53` (must redraw), `between_f64(x, x)`, `Weights` with zero-weight entries,
   `TotalOverflow`.
6. **`Uuid.roc`, `UuidV7.roc`.** RFC 9562 Appendix A v4 and v7 examples
   (layout and bits); `uuid` crate round trips and rejected strings
   (wrong length, braces, `urn:uuid:`, non-hex, misplaced hyphens); ordering
   of `Uuid` equals byte order; a JSON record round trip. `UuidV7` expects:
   first call at `now = 0` seeds rather than increments, same millisecond strictly increasing, clock going backwards,
   counter overflow advancing `last_ms`.
7. **`package.toml`** (`[deps]` and `[dev-deps]` both name trantor-cli, as
   trantor-files does), **README, `tests/readme`**; `trantor test .` passes.
   The README's v7 example converts `Utc.to_millis_since_epoch` (a `U128`,
   trantor-cli `Utc.roc:12`) to `U64`.
8. **trantor-cli**: one line in `components/basic-lib/Random.roc`'s module doc
   pointing to trantor-random for anything beyond a seed. Run its surfaces
   test.
9. **trantor docs**: D-S1-11 item 4 marked designed and implemented; the
   upstream gaps note gains the generic-body finding (D-S4-8); record
   implementation notes here.

## Implementation notes

Repo `~/dev/roc/trantor-random` (new, branch `main`): `69acbbc` vectors from
Go, `70938ca` `Rng` and `FastRng` with the gate, `bad52b5` saved state,
`1e5d3a3` `RngDraw` and `Weights`, `b596eeb` `Uuid` and `UuidV7`, `1e48c22`
`RngStable`, `ef0d596` README, `9e189a4` manifest comments. trantor-cli branch
`trantor-random-pointer` (worktree `.claude/worktrees/trantor-random-pointer`), `561b161`:
the `Random.roc` doc line; its suite passed, with one earlier run failing `tests/confined-race` (append, open_writer) and passing on rerun with no change. trantor-random's `trantor test .` passes: 300 expects (66 the
package's own), `tests/readme` exact, `tests/vectors` 2 cargo tests.

**Gate (D-S4-12), passed.** `python3 bench/alloc/run.py`, roc `10e922df83`,
`--opt=speed`, arm64 macOS:

| Path | Allocations after construction |
|---|---|
| `Rng.next_u64`, 1M draws (about 31k refills) | 0 |
| `FastRng.next_u64`, 1M draws | 0 |
| `RngDraw.below(1000)` on `Rng`, 1M draws | 0 |
| `RngDraw.below(1000)` on `FastRng`, 1M draws | 0 |
| `UuidV7(Rng).next`, 500k UUIDs | 0 |

`Rng` 8.89–8.90 ns per `u64`, `FastRng` 1.21–1.27 ns, ratio 7.0–7.4× over
three runs of 10M draws (fastest of 5 per build, minus an empty build). Under
the 10× stop line.

Details within the plan's design:

- **References.** Go's output agrees with the C2SP sample (372 words, taken
  from the C2SP repo's `chacha8rand.md`, checked in as
  `tests/vectors/c2sp/sample-output.hex`) before anything is generated.
  `go/golden.txt` is committed; `cargo run -- --regen-go` rewrites it. The Go
  program never calls `Read`.
- **Layout changes, forced by tooling.** The gate lives in `bench/alloc`, not
  `tests/alloc`: `trantor test` treats every `tests/<name>` as a suite (app,
  cargo, or `test.sh`), and this one needs the roc repo's platform. The draw
  checks through `Rng`'s and `FastRng`'s methods live in a component-only
  `RngDrawCheck.roc`: `Rng` imports `RngDraw`, so `RngDraw.roc` cannot import
  `Rng`. `Uuid.roc` and `UuidV7.roc` hold their own vector checks (they may
  import the generators; nothing imports them back).
- **`RngStable.roc`** holds hand-copied outputs: words across the first
  reseed, `FastRng.from_u64(42)`, a saved state at 96 words, one output of each
  draw on each generator, one v4 and one v7 string. Mutating `below`'s
  threshold, `shuffle`'s bound, `UuidV7`'s overflow test and v7's `rand_a`
  shift each failed the matching checks, and the restored code passed.
- **`Weights.index_for`** takes `x` modulo the total, so it has no failure
  case; `pick` always passes a value below the total.
- **`FastRng.from_os!`** falls back to `from_u64` of the first word if all four
  OS words are zero (probability 2^-256), since `from_words` refuses that
  state.
- **`Uuid.v7`** masks `unix_ms` to 48 bits as specified; `UuidV7.next` crashes
  at 2^48 as specified. The two differ on purpose and both are documented.
- **Public docs do not mention Go** (D-S4-10): README and `##` comments call
  saved state opaque. `ChaCha8.roc`'s module comment names the Go file it is
  ported from, and `Rng.refill` names Go's `Refill`; those describe the
  algorithm, not the state format.
- **Not checked:** `Hash.of(uuid)` through trantor-hash (no trantor-hash
  dev-dep). `encoder_for` is format-generic as trantor-hash requires; the
  README does not claim hashing.
- **Compiler findings** (generic-body missing methods, nominal field decoding
  through `Json.parse`, parameter destructuring of nominal records, names
  shadowing methods) are recorded in `notes/2026-09-14-upstream-builtin-gaps.md`.

**Independent code review** (one Opus reviewer, after implementation). No
defect in the generators, saved state, draws or UUIDs. Its findings, all
fixed:

- `FastRng.fork` gave the same child for repeated forks of one parent; changed
  with the user to child = copy, parent jumped (D-S4-10), trantor-random
  `90264f8`. The fork expect now also forks the parent again.
- `Rng.from_u64` was pinned by nothing: a changed expansion passed every
  expect. `RngStable` also lacked `u8`, `bool`, `u64`, `between_u64`
  (including full range), `choose`, `fork`, `FastRng.to_bytes` and `UuidV7`
  values. Added in `bc4e50f`; `Rng.from_u64(0)` and `(7)` come from a separate
  Go SplitMix64 feeding Go's ChaCha8. The reviewer's mutation now fails two
  expects.
- Doc comments: `between_f64` with equal infinite bounds returns without
  crashing; a `Uuid` in a record does not decode through `Json.parse` (`bc4e50f`).
- trantor-cli `confined-race`: the de-flake (`3a5a5d6`, from a suite run where
  append and open_writer saw 0 successes in 2000) started its 10 s before the
  2000-attempt minimum and waited forever if the clock read 0. Fixed in
  `e877f93`: the budget starts after the minimum, a 0 clock ends it, cap 300 s.
  Checked with the test's swapper on an idle and a fully loaded machine, a
  swapper that stalls 1.5 s at a time (passes; `copy_dir` needed up to 48,903
  attempts), and one that never swaps (every op still fails its refused
  control, 122 s). The full trantor-cli suite passes.

Not done: the main trantor checkout still holds untracked copies of this plan
and the S4 log and the uncommitted S1 edit from before the worktree existed.
This session was confined to the worktree and could not remove them.

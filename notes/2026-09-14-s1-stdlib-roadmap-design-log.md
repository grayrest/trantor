# S1 — standard library roadmap: design log (2026-09-14)

**Survey:** [`notes/2026-09-14-stdlib-survey.md`](2026-09-14-stdlib-survey.md).
**Plan:** [`plans/2026-09-14-trantor-hash.md`](../plans/2026-09-14-trantor-hash.md).
**Upstream gaps (not pursued):** [`notes/2026-09-14-upstream-builtin-gaps.md`](2026-09-14-upstream-builtin-gaps.md).

Settled against trantor `f487487`, roc `local-fixes` `10e922df83`. The brief:
trantor packages have become the standard library for trantor Roc. The survey
listed candidate additions from Rust, Python, Elixir and Clojure; this session
decided which of them trantor takes on, where they live, and in what order.
Only the first step, `trantor-hash`, is designed here. Each later step is
grilled on its own when it starts (D-S1-13).

## What was found before any question was asked

- **Pure packages already work.** trantor-terminal declares `terminal-keys` and
  `terminal-width` as `kind = "roc"` components with no host half. A package
  made only of such components needs nothing new from the package model.
- **The compiler fork carries two hashing patches.** On `local-fixes`
  (unpushed, 7 commits over `697ada7ba3`): `2077175963` adds `Hasher` to the
  auto-import scope list in `src/canonicalize/Can.zig` and gives it
  `new`/`finish`; `205f49cb94` adds `Hasher.hash_of(value, seed)`. `7d864f0352`
  edits that doc comment, and `10e922df83` refreshes a builtin node-id
  snapshot. upstream `origin/main` (`7bac174fd7`, 2026-09-14) still does not
  let user code name `Hasher`, so a package cannot reach derived `to_hash`.
- **Three consumers.**
  - `tower-platform/platform/Req.roc:114,173`: `Hasher.hash_of(decoded,
    Host.hash_seed!())` over any decoded body or query type, used as the
    response-cache key. `Req.roc:480-486` has expects calling
    `Hasher.new(...).finish()` directly.
  - `roc-solid/crates/host-im/roc/Id.roc:194` and the same line in
    `roc-solid-eink`: `U64.to_u32_wrap(Hasher.hash_of(s, 0))` as an element
    id's local part.
- **Derived `to_hash` blows up at compile time; derived `encoder_for` does
  not.** `roc-solid-eink/notes/2026-07-28-HANDOFF-roc-agent-to-hash-compile-blowup.md`:
  on the conduit model shape, K=5 sub-models took 106 s for `to_hash` against
  1.35 s for an `encoder_for` traversal; the real conduit model was killed at
  95.4 GB. The probes in `roc-solid-eink/notes/probes/tohash/k*-enc.roc`
  already hash through a userland format.
- **The tower cache key is attacker-facing.** Request bodies are client input.
  Two bodies that collide share one cached response, so a crafted collision is
  cache poisoning, not only a slow table
  (`tower-platform/plans/2026-07-17-input-extractors-cache-keys.md:88`).
- **Neither tower-platform nor roc-solid's host-im platform is composed by
  trantor.** roc-solid-eink is; roc-solid has one trantor world
  (`platform/clay/world-vello.toml`) but `Id.roc` belongs to host-im.
- **A CSV package exists.** `~/dev/roc/playground/csv`: about 2,000 lines of
  pure Roc, CSV and TSV, typed decode and encode as `Encoding` formats, cell
  errors with line and column.

## Decisions

### D-S1-1 Method-shaped gaps belong upstream; trantor owns new types and domains

Gaps in builtin types (`F64` logs and exponentials, `Str` code point iteration
and `find`, the `List` vocabulary) are upstream's. trantor owns new types and
domains: sorted maps, formats, PRNGs, hashing. (User.)

**Why:** method-call syntax makes builtin methods far better to call than a
parallel `ListX` module, and a parallel vocabulary would outlive the upstream
fix. New types and formats have no upstream home.

**Rejected:** upstream only (new domains would have nowhere to go); trantor
only (`ListX.group_by(xs, f)` forever, dead once upstream adds it).

### D-S1-2 Retire the compiler hashing patches with a trantor package

`trantor-hash` replaces `Hasher.new`, `Hasher.finish` and `Hasher.hash_of`, and
the four hashing commits leave `local-fixes`. (User.)

### D-S1-3 Hash values through an `Encoding` format

The package defines a hashing format; derived `encoder_for` walks the value and
the format feeds each leaf into a byte hash written in pure Roc. (User.)

**Why:** no compiler patch; follows the traversal already measured to compile
normally where derived `to_hash` does not; no intermediate byte buffer.

**Rejected:** JSON-encode then BLAKE3 (a buffer per hash, and field renames or
number formatting change the key); a Rust host component (still needs the bytes
from one of the other two, plus a host crossing per hash).

### D-S1-4 Two hashes: keyed SipHash-1-3 and fast rapidhash; 128-bit later

`Hash.of` is SipHash-1-3 with a 128-bit key, for anything crossing a trust
boundary. `Hash.fast` is rapidhash with a `U64` seed, for ids and tables. Both
return `U64`. A 128-bit output is deferred. (User.)

**Why:** tower's key needs a keyed hash, since crafted collisions are cache
poisoning. SipHash is Rust's default `HashMap` hasher for that reason, and its
128-bit key is cheap. roc-solid's ids are not attacker-facing and hash short
strings.

**Rejected:** SipHash with a `U64` seed (halves the key strength to keep
tower's host interface); rapidhash alone (seed-independent collisions are
published for the family); FxHash (poor low bits, and `Id.hash` keeps the low
32).

### D-S1-5 `Hash.fast` is rapidhash v3

(User.)

**Why:** small, and its core 64×64→128 multiply maps onto `U64.to_u128` and
`times`. The format feeds short leaves, almost all under 16 bytes, where xxh3's
SIMD path (which Roc can compile, as `~/dev/roc/regex` shows) gives nothing.
xxh3 remains the candidate for a later `Hash.fast_bytes` over large buffers.

### D-S1-6 Hash values are stable within a major version

Both algorithms are checked against their reference vectors. The structural
layout (leaf encodings, tag names, separators) is pinned by expects over exact
hashes of sample values; changing it is a major version bump. (User.)

**Why:** someone will persist an `Id.hash` value, and a version bump is the
honest signal. Freezing the layout forever would lock in an unreviewed first
draft.

**Rejected:** no promise; stable forever.

### D-S1-7 One package per domain

Additions ship as small packages, one per subject a user would name in a
dependency line: `trantor-hash`, `trantor-encoding`, `trantor-text`, and so on.
(User.)

**Why:** matches trantor-terminal and trantor-temporal; Roc compiles everything
a program pulls in; a pure package needs no host and no `trantor-cli`.

**Rejected:** a few broad packages; one `trantor-std`.

### D-S1-8 Platforms outside trantor vendor the modules, then migrate

tower-platform and roc-solid's host-im platform take pinned copies of the
`trantor-hash` modules, with the source commit in a header comment. They switch
to a real dependency when they are composed by trantor. (User.)

**Why:** each needs one small module, and the copies retire the compiler patches
without waiting for those migrations.

**Rejected:** importing the source by relative path (depends on checkout
layout); migrating both platforms first (patches stay until then); a
`trantor vendor` command (tooling for a need that ends at migration).

### D-S1-9 Gate for dropping the compiler patches: consumers build and pass

The hashing commits leave `local-fixes` once tower-platform, roc-solid and
roc-solid-eink build and pass their tests with the vendored modules on a
compiler without them. No benchmark comparison is required. (User.)

**Rejected:** also benchmarking tower's cached-request path and rebuilding
conduit for compile time and memory; removing first and fixing what breaks.

### D-S1-10 The `trantor-hash` surface

```roc
Hash :: [].{
    Key : { k0 : U64, k1 : U64 }
    of : a, Key -> U64 where [a.encoder_for : ...]     # SipHash-1-3
    fast : a, U64 -> U64 where [a.encoder_for : ...]   # rapidhash v3
}
SipHash :: ...    # new(Key), write(List(U8)), write_u64, finish -> U64
RapidHash :: ...  # new(U64), write(List(U8)), write_u64, finish -> U64
```

(User.)

**Why:** names say what each hash is for, so choosing does not require knowing
which algorithm resists attack; the docs name the algorithm and the stability
promise. `Key` is one record so k0 and k1 cannot be swapped. The incremental
hashers are public because the format is built on them and custom hashing needs
nothing more. No `Str` fast path: a `Str` is one leaf write.

**Rejected:** algorithm names (`Hash.sip`, `Hash.rapid`); internal-only
incremental hashers.

### D-S1-11 Roadmap

1. `trantor-hash`
2. `trantor-cli` host gaps: temp files, copy, recursive walk and glob, append
   and streaming writes, subprocess spawn with pipes *(designed in D-S2; split
   into trantor-cli primitives, `trantor-files` and `trantor-process`,
   D-S2-18..22; implemented, see the S2 plan's notes)*
3. `trantor-encoding`: Base64 and hex, CSV, TOML *(designed in D-S3, see
   `notes/2026-09-16-s3-encoding-design-log.md`; implemented in
   `~/dev/roc/trantor-encoding`, see the S3 plan's notes)*
4. `trantor-random`: host-seeded PRNG, UUID *(designed in D-S4, see
   `notes/2026-09-16-s4-trantor-random-design-log.md`; implemented, see the
   plan's notes)*
5. `trantor-text`: Unicode properties, case folding, normalization, wrapping,
   string distance, diff; grapheme segmentation moves here from
   trantor-terminal
6. `trantor-collections`: sorted map and set, deque, heap, topological sort

(User.) Out of scope: **roc-weaver** (not the user's package to control),
**regex** (kept independent of trantor), **`trantor-crypto`** (skipped; HMAC,
constant-time compare and secure random bytes go with it). `trantor-text` was
moved ahead of `trantor-collections`.

**Why this order:** existing code and what live apps (trantor itself, rocjust,
seahaven, tower, roc-solid) would use now come first.

### D-S1-12 CSV moves into `trantor-encoding`

`~/dev/roc/playground/csv` becomes a component of `trantor-encoding`, not a
package of its own. (User.)

This is the one exception to D-S1-7: encoding is treated as the subject, and
CSV sits beside Base64, hex and TOML.

**Rejected:** a separate `trantor-csv`; keeping it independent like regex.

### D-S1-13 Upstream gaps are documented and not pursued; each step gets its own grill

The method-shaped gaps from D-S1-1 are listed in
`notes/2026-09-14-upstream-builtin-gaps.md`. The trantor effort sends no PRs,
carries no fork patches and ships no stopgaps for them. (User.)

Steps 2–6 are designed in their own grill sessions when each starts, from the
code they touch (`fs-confined` preopens, `sync-io`, the TOML `Encoding` shape,
the UCD tables). (User.)

### D-S1-14 rocjust is not migrated as part of this effort

No step moves rocjust onto trantor packages: not its pipe (S2's `tests/spawn`
covers the shape it fakes), not `Tty.is_terminal!` (K1), not a release gate.
(User.)

**Why:** rocjust runs on seahaven's execution environment, which would have to
move to trantor first, and a release does not need either. rocjust stays a
source of real usage for design, as D-S1-11's order already treats it; H5's
remaining work ("`rocjust` green") is likewise not required for a release.

**Rejected:** migrating rocjust with S2 or K1; making it a release exit.

### D-S1-15 The family moves to roc `d861ad`: no `..` in an output position

`~/.bin/roc` and `RustGlue.roc` were replaced on 2026-09-25 with
`release-fast-d861ad68`. Under it a tag union in an output position is
implicitly open, so an explicit `..` there is a warning — and trantor treats a
`roc check` warning as a failure, so nothing in the family built. A freshly
scaffolded app over an untouched trantor-cli produced 68 of them.

**This is a migration, not a cleanup.** Removing a return-position `..` CLOSES
the union on the previous compiler: measured on `roc-554a76-rebased-main`,
`validate_ipv4 : Str -> Try(Str, [InvalidIpv4(Str)])` stops unifying with
`Try(Str, Url.ParseErr)`. The swept family therefore builds only on `d861ad`,
and there is no going back short of reverting the sweep.

What was swept, and how each site was found:

- **241 sites** in the packages' components and interfaces, taken from the
  compiler's own `file:line:col` rather than a pattern, and applied bottom-up.
- **12 sites** in test apps and one bench app, by a depth-aware rule: only after
  the last top-level `=>`/`->`. The five like
  `outcome : Try(a, [PathErr(IOErr), ..]) -> Str` are ARGUMENTS and were left.
- **106 signature snippets** in the packages' READMEs, so the documented API
  still matches the code.
- **27 sites** in this repo's golden fixtures, by the same rule.
- `main_contract.rs`: `trantor new` copied the platform's `requires` entry
  verbatim, so every scaffolded app inherited a `..` and warned on its own first
  check. The copy is trimmed now; `close_output_unions` leaves an argument's
  `..`, a named extension, and a lone `[..]` (which would otherwise become `[]`
  and mean the opposite).

**A `requires` block keeps its `..`, and that is not negotiable.** It is the
contract an app must satisfy, not an output position: closed, it refuses an app
whose `main!` returns a tag of its own — `argv-bytes` failed exactly that way,
`[StdoutErr(IOErr)]` against a `[Io(IOErr)]` requirement. The compiler warns
there anyway, and both remedies its message offers are unavailable: `..others`
is "Type variables must be introduced in a type annotation before they can be
used", and `[err] for main! : ...` fails to parse the header. The ten fixture
`driver.toml`s and the two expected-output `golden/platform/main.roc` files
keep theirs for the same reason.

**Four things are red on `d861ad`, and a control proves the sweep caused none
of them:**

1. `trantor-encoding`'s gate dies in `roc test` with
   `RuntimeHostEnv: double-free or untracked free` (SIGABRT). Reproduced with
   the package's own sweep reverted and everything else held constant, so it is
   a compiler bug. Its `tests/` suites — including the toml-test conformance
   vectors — cannot run on this compiler at all.
2. `b8-basic-cli` — SETTLED, see below; the fixture is green again.
3. `b8-basic-cli`'s `url` — NOT the compiler, and not the sweep. See below.
4. `seahaven-slice` asserts its `stdio-lib/Stdout.roc` is byte-identical to
   seahaven's. Sweeping the copy breaks that assertion; keeping it fails the
   build on the warning. The copy is kept identical, so the fixture records
   that seahaven has not migrated.

Green: eight packages (cli, files, process, net, terminal, temporal, hash,
random), 19 of 20 fixtures, 135 unit tests, clippy clean.

**`b8-basic-cli`, settled the same day.** Two causes, neither the sweep's:

- `file-size` spells an explicit `..` in a return type, in basic-cli 0.21.0's
  own source. `$TAG` is a released tag whose contents never change, so it will
  carry that spelling forever and the file is not ours to edit — the fixture's
  premise is that upstream examples check UNMODIFIED. The row now passes when
  the example typechecks and the only complaint is that one lint, matched by
  name; anything else still fails it. `roc build` needed the same reading: it
  prints "successfully building" and writes the binary, then exits 2.
- `url` has not typechecked since `2bb1528`, which is ours:
  `append_path_segments` returns a `Try` so it can refuse a `.` or `..` item
  (D-S2-63), and the example chains `.append_query_param` onto its result.
  Confirmed on the PREVIOUS compiler with the PRE-sweep package — the same
  three `missing method` errors — so neither `d861ad` nor the sweep is
  involved. It is excluded from the proof as the http examples are, and the
  divergence is ASSERTED: if `append_path_segments` stops returning a `Try`,
  the fixture fails and says to put `url` back. Nobody had noticed because the
  review rounds ran the package gates, not this repo's `just verify`.

Found while excluding it: the fixture never cleaned its staged binaries, so
`url`'s `expect` row had been green since 2026-09-18 against an `ex-url` built
before the divergence it cannot compile against. Any example dropped from the
build list would have done the same. The binaries are removed before the build
loop now.

(User, 2026-09-25, after the sweep was priced: the one-way cost and the four
red items were on the table before it landed.)

### D-S1-16 The family runs on roc `ec5981`, and every gate is green

`~/.bin/roc` became `release-fast-ec598185` on 2026-10-22. The family's last
move had been to `620d04` on 2026-10-12, and that move was partial. A run of
every gate on 2026-10-25 found five packages red. Each failure was then run on
`620d04` as a control, which split them by cause:

| Package | On `620d04` | Cause | Fix |
|---|---|---|---|
| files, random | red | the `620d04` sweep (`import X exposing [X]` is an error) never reached them | the same sweep |
| temporal | red | a lint added between `d861ad` and `620d04`: an `if` on a comparison the compiler decides is "unconditional", and a warning fails `roc check` | `tests/behaviours` goes through its own `yn` helper; the line still checks `eq=no` |
| encoding | red | a compiler refcount bug, below | `TomlTree.slot_at` |
| terminal | green | `ec5981`: a union a host function returns cannot widen | rebuild at the device boundary |
| cli | green | `ec5981` frees a value at its last use, earlier than before | a test-app fix, not a package one |

**A host's union is rebuilt where it enters Roc, not where it is widened.**
Under `ec5981` a tag union a host function returns is exactly that union. It
cannot join a caller's larger one, and `?` hands it on unchanged, so
`Ok(TerminalDevice.write!(..)?)` only looked like a rebuild. Five sites broke,
and trantor's b8 fixture broke through them. trantor-terminal now rebuilds
every device error in the wrapper that makes the call. That includes `open!`,
`size!` and `suspend!`, which nothing inside the package widens. They are
public, and a consumer that uses `?` on one into its own wider error type would
hit the same error. No other package has a wrapper of this shape.

**Holding a resource in a list does not keep it alive.** Roc frees a value
after its last use, not at the end of its scope. `fd-handoff` held 61 open
descriptors in a list it never read again. `ec5981` closed all 61 on entering
the arm that used up the fds, and `620d04` already did the same once the list
left the loop. The test now hands each descriptor to the host after the
handoff. Any code that relies on a list to keep resources open has the same
exposure. That is not a compiler bug, since the language promises nothing
about when a value is freed.

**trantor-encoding: a compiler bug, worked around rather than waited on.**
Three things together trigger it:

- a `match` that binds a nested `Ok(Tag(record))` straight off `get`, `first`
  or `last`;
- a union with more than one variant;
- a payload holding a refcounted field that the arm then consumes.

The binding is never shared, so `record.path.append(..)` writes over a list
the source still owns. The interpreter is correct. The dev and LLVM backends
are not, on every build since `d861ad`. That is why the gate could not run at
all from 2026-09-25 (D-S1-15's first red item). The crash also took
trantor-temporal's formats suite with it, because that suite writes
`[outer.event]`. The standalone reproduction, with no trantor involved:

```roc
Repro :: [].{}

Slot : [Full({ level : U64, path : List(U64) }), Empty]

build : List(Slot) -> List(Slot)
build = |slots| slots.append(Full({ level: 2, path: [].append(1) }))

expect {
	slots = build([])
	extended =
		match slots.get(0) {
			Ok(Full(record)) => record.path.append(2)
			_ => []
		}
	extended == [1, 2]
}
```

`roc test --no-cache Repro.roc` gives `[0, 2]` on `ec5981`, SIGSEGV with
`--opt=speed`, "List written in place while another reference to it was live"
from the debug build, and `RuntimeHostEnv: realloc of untracked memory` on
`620d04` and `d861ad`. Matching in two steps (`Ok(slot) => match slot`) passes
everywhere. That is the workaround, at the seven `TomlTree` sites that had the
shape. No other site in the package has it.

**Open: `roc test` sometimes hangs on a cache hit.** On trantor-encoding's
composed expects, `ec5981` with the shared cache hung in about 4 runs of 11.
It used 100% CPU on one thread inside the compiler and printed nothing, even
with `--timings`. With `--no-cache` it hung in 0 runs of 15, and each of those
runs compiled from scratch and passed all 1424 expects. A fresh
`ROC_CACHE_DIR`, and an APFS clone of the shared cache directory, also passed.
So the cached path is at fault, not our code. A gate that hangs this way is
killed at `TRANTOR_TEST_TIMEOUT` (900 s) and reads as a failure. No other
package's expects have hung. The `ec5981` debug build cannot help here: on the
full composed platform it aborts on a postcheck invariant first
("stored const representation differed from its expected Monotype type").

Verified 2026-10-26 on `ec5981`: all nine package gates pass (encoding's with a
fresh `ROC_CACHE_DIR`), `just verify` runs 20 of 20, trantor's 135 unit tests
pass and clippy is clean, and seahaven's platform checks clean.

(User, 2026-10-25: "get all repos running".)

## Still open

- Moving the compiler fork's `local-fixes` off the hashing commits, and
  merging the three consumer branches, wait on the user (plan step 9). The
  structural layout is pinned (trantor-hash `3140140`) against a Rust model of
  the documented table.

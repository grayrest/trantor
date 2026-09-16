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
3. `trantor-encoding`: Base64 and hex, CSV, TOML
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

## Still open

- Moving the compiler fork's `local-fixes` off the hashing commits, and
  merging the three consumer branches, wait on the user (plan step 9). The
  structural layout is pinned (trantor-hash `3140140`) against a Rust model of
  the documented table.

# Standard library survey — Rust, Python, Elixir, Clojure

Date: 2026-09-14. Status: survey. Decisions taken from it are in
[`2026-09-14-s1-stdlib-roadmap-design-log.md`](2026-09-14-s1-stdlib-roadmap-design-log.md)
(D-S1); the builtin gaps are split out into
[`2026-09-14-upstream-builtin-gaps.md`](2026-09-14-upstream-builtin-gaps.md).

Trantor packages are acting as the standard library for trantor Roc. This note
lists candidate additions found by comparing four mature standard libraries
against what a trantor app can already reach. It is input to later grill
sessions; nothing here is decided.

## Method

- Surveyed from current docs: Rust `std` (doc.rust-lang.org), Python 3.14
  (py-modindex), Elixir 1.20 plus the Erlang/OTP modules Elixir code calls
  directly (hexdocs), Clojure 1.12 core, bundled namespaces and de-facto contrib
  (clojure.github.io, cheatsheet).
- Baseline: builtins of roc `10e922df` (function names read from
  `~/Repositories/roc/generated-docs`), plus trantor-cli, trantor-net,
  trantor-temporal, trantor-terminal, and the libraries under `~/dev/roc`.
- Dropped as not applicable: language machinery (reflection, macros, pointers,
  interior mutability, operator traits, exceptions), BEAM/JVM process models,
  and platform-specific syscalls.

Source tags: **R** Rust, **P** Python, **E** Elixir/OTP, **C** Clojure.
Kind: **pure** (a Roc library, no interface) or **host** (needs an interface
and a Rust host component).
Priority is a first guess at everyday value for CLI tools and services:
**1** most programs hit it, **2** common, **3** specialised.

## What already exists

| Area | Where |
|---|---|
| Str, List, Dict, Set, Iter, Try, numerics (checked/wrapping/saturating ops, bit counts, `from_str`, trig, `sqrt`, `pow`) | builtins |
| Sorting (`sort`, `sort_by`, `sort_with`, reversed variants) | builtin `List` |
| JSON codec, HTTP header codec, format-agnostic `Encoding` | builtin |
| SHA-256, BLAKE3 with hex digests | builtin `Crypto` |
| Paths, filesystem (read/write/list/rename/hard link/timestamps/delete tree), env, args, cwd, exit, stdio | trantor-cli |
| Subprocess (run to completion: status, output, inherit stdin) | trantor-cli `Cmd` |
| Wall and monotonic clock, sleep, entropy seeds | trantor-cli `clocks`, `random` |
| URL parse/resolve, percent and form encoding, query pairs | trantor-cli `Url` |
| BCP-47 locale tags | trantor-cli `Locale` |
| HTTP client (TLS optional), TCP connect/listen/accept, UDP, DNS resolve | trantor-net |
| Dates, times, zones, durations, calendars, strftime | trantor-temporal |
| Raw mode, key/mouse decoding, ANSI, screen diffing, grapheme width | trantor-terminal |
| Regex over bytes (RE#) | `~/dev/roc/regex`, not packaged |
| CLI argument parsing (clap-style) | `~/dev/roc/roc-weaver`, not packaged |
| CommonMark + GFM | `~/dev/roc/roc-markdown`, not packaged |
| SQLite | `roc:sqlite-unsound` spike, blocked on clone-on-incref |

## Candidates

### Promote what already exists

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Regex package | R(crate) P E | pure | 1 | `~/dev/roc/regex` exists; needs a `Str` API (named captures, replace, split, scan) on top of the byte matcher |
| CLI argument parsing | P E C | pure | 1 | roc-weaver exists; package it |
| Grapheme segmentation as its own package | E | pure | 2 | `terminal-width` already carries UCD tables; text code shouldn't depend on a terminal package |
| SQLite | P | host | 2 | waits on upstream clone-on-incref |

### Text

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Code point iteration and `Char`-like properties (alphabetic, numeric, whitespace, case) | R E | pure | 1 | `Str` iterates bytes only; no code point view |
| Substring search: `find`, `rfind`, `index_of`, all matches | R P E | pure | 1 | `Str` has `contains` and `split_first` but no positions |
| `splitn`/`rsplitn`, `lines`, `split_whitespace` | R P C | pure | 1 | `split_on` only |
| Padding and alignment: `pad_start`, `pad_end`, `center` (width-aware) | R P E | pure | 1 | uses grapheme width |
| Number formatting: radix, fixed precision, thousands separators, sign | R P | pure | 1 | `to_str` only; no `{:.2}` or `{:x}` equivalent |
| Integer parsing with radix | R P E | pure | 2 | `from_str` is decimal only |
| Full Unicode case mapping and case folding | R P E | pure | 2 | builtins are ASCII-only |
| Normalization (NFC, NFD, NFKC, NFKD) | P E | pure | 2 | same UCD pipeline as width |
| `textwrap`: wrap, fill, indent, dedent, shorten | P | pure | 2 | width-aware wrapping |
| String distance: Levenshtein, Jaro, "did you mean" close matches | P E | pure | 2 | CLI suggestion messages |
| Sequence and text diff (Myers), unified diff output | P E C | pure | 2 | Elixir `myers_difference`, Python `difflib`, Clojure `data/diff` |
| Shell word splitting and quoting | P | pure | 2 | pairs with `Cmd` |
| Glob / fnmatch pattern matching (pure) | P E | pure | 2 | the directory walk is the host half, below |
| Simple string templating | P | pure | 3 | Roc interpolation covers most uses |

### Numbers

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Float math gaps: `exp`, `ln`, `log2`, `log10`, `atan2`, `hypot`, `cbrt`, float-valued `floor`/`ceil`/`trunc`/`round`, `fract`, `copysign`, approximate equality | R P E C | pure* | 1 | F64 lacks logs and exponentials entirely; *may belong in builtins (upstream) |
| Integer gaps: `gcd`, `lcm`, `isqrt`, `digits`, big-endian byte conversion, `rem_euclid` | R P E | pure | 2 | little-endian only today |
| PRNG algorithms (PCG or xoshiro) seeded from the host: ranges, floats, shuffle, choice, sample, weighted choice | R P E | pure | 1 | trantor `Random` only yields seeds |
| Statistics: mean, median, mode, variance, stdev, quantiles | P | pure | 2 | |
| Arbitrary-precision integers | P E(native) | pure | 3 | no bignum; `Dec` is fixed 128-bit |
| Rationals | P | pure | 3 | |

### Collections

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| List vocabulary: `windows`, `dedup`/`distinct`, `group_by`, `frequencies`, `partition`, `chunk_by`/`partition_by`, `take_while`/`drop_while`, `zip`/`unzip`, `min_by`/`max_by`, `scan`/`reductions`, `interleave`, `rotate` | R P E C | pure | 1 | builtins have `chunks_of`, `split_if`, `map2`, `fold_until`, but none of these |
| Binary search, `partition_point`, `insort` | R P | pure | 2 | sorted `List` has no search |
| Dict extras: `merge_with`, `update_keys`/`update_vals`, `get_or`, `group_by` into Dict, nested path get/update | E C | pure | 2 | `Dict.update` and `insert_all` exist; collisions are last-write-wins |
| Sorted map and set with range queries | R E C | pure | 1 | `Dict`/`Set` are hash-only; no ordered iteration |
| Deque / FIFO queue (amortised O(1) both ends) | R P E | pure | 2 | Erlang `:queue`, Okasaki banker's queue |
| Priority queue / heap | R P C | pure | 2 | Dijkstra, schedulers, top-k |
| Multiset / Counter | P | pure | 3 | mostly `frequencies` plus arithmetic |
| Combinatorics: product, combinations, permutations | P C | pure | 3 | |
| Topological sort / dependency graph | P | pure | 2 | trantor itself resolves component graphs |
| Zipper over trees | C | pure | 3 | useful with markdown/HTML trees |

### Encodings and formats

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Base64 (standard and URL-safe), Base32, hex for arbitrary bytes | R(crate) P E | pure | 1 | only digests have hex |
| Binary reader/writer: fixed-width ints both endians, varints, length-prefixed fields | P(struct) E(binary) | pure | 2 | |
| CSV (RFC 4180) read and write, header-keyed records via `Encoding` | P C | pure | 1 | |
| TOML read (and write) via `Encoding` | P | pure | 1 | trantor's own manifests are TOML |
| Semver parse, compare, requirement matching | E | pure | 2 | trantor pins and registry already need it |
| UUID (v4 random, v7 time-ordered) | P | pure | 2 | built on PRNG + clock |
| IP address and CIDR types | R P | pure | 2 | `Url` validates IPv4/IPv6 internally; expose it |
| HTML escape and unescape | P | pure | 2 | |
| MIME type table by extension | P | pure | 3 | |
| XML parse and build | P | pure | 3 | |
| INI | P | pure | 3 | |
| Pretty-printer for nested values (width-aware) | P C E | pure | 3 | `Str.inspect` is single-line |

### Hashing and crypto

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| HMAC (over SHA-256/BLAKE3) and constant-time compare | P E | pure | 1 | webhook signatures, signed cookies |
| SHA-1, SHA-512, MD5 | P E | host or pure | 2 | interop only (git, legacy checksums) |
| Non-crypto hashes: CRC32, xxHash | R(crate) E | pure | 2 | needed by gzip/zip |
| Secure random bytes and tokens | P | host | 1 | `random` gives seeds, not a CSPRNG byte stream |
| Password hashing (argon2id), AEAD encryption | E | host | 3 | wrap vetted Rust crates, never hand-roll |

### Filesystem and processes (host)

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Temp files and directories, cleaned up by scope | R P | host | 1 | |
| Copy file and copy tree | P E | host | 1 | `rename` and `delete_all` exist; `copy` doesn't |
| Recursive directory walk and glob | P E | host | 1 | `list!` is one level |
| Append and streaming writes | R P E | host | 1 | writes are whole-file today |
| Symlinks (create, read link, lstat), canonicalize | R P | host | 2 | `resolve!` may cover canonicalize; check |
| Permission bits (read and set), file times (set) | R P | host | 2 | `is_executable!` is read-only |
| Subprocess spawn with piped stdin/stdout/stderr, streaming, wait, kill | R P E C | host | 1 | `Cmd` runs to completion only |
| Signal handling (at least SIGINT/SIGTERM as events) | P | host | 2 | Rust std lacks it too |
| Home, temp and config directories (XDG / platform) | R P | host | 2 | |
| Hostname, OS/arch info, CPU count | R P E | host | 3 | |
| Password prompt without echo | P | pure | 3 | buildable on trantor-terminal raw mode |

### Networking (host)

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Streaming HTTP request and response bodies | P E | host | 2 | `plans/2026-09-04-http-streaming-client.md` covers this |
| TLS for raw TCP | R(crate) P | host | 3 | HTTP has optional rustls; `Tcp` doesn't |
| Unix domain sockets | R P | host | 3 | |
| HTTP server | P | host | 2 | lives in tower-platform, not a trantor package |

### Compression and archives

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| gzip / deflate / zlib | P E | host (or pure) | 2 | the ureq host already links a gzip decoder |
| zip and tar | P | pure over deflate | 3 | tar is pure; zip needs deflate + CRC32 |
| zstd, brotli, xz | P | host | 3 | |

### Program-level utilities

| Candidate | From | Kind | Pri | Note |
|---|---|---|---|---|
| Leveled, structured logging to stderr (text and JSON lines) | P E | pure | 1 | over `Stderr` + clock |
| Paginated effects as an iterator (`iteration`) | C | pure | 3 | cursor-driven APIs as an `Iter` |
| Memoised effect with eviction | C | pure | 3 | |

## API design ideas worth carrying over

- **Subject-first arguments** (Elixir): the value being operated on is always
  the first argument, so method-call and pipe chains read left to right. Roc's
  static dispatch already rewards this; make it a rule for every package.
- **One naming family for update-with-default** (Elixir `Map.update/4`,
  `get/3`, `put_new/3`): the same shapes across Dict, sorted map, and Counter.
- **Early-exit folds by tag** (Elixir `reduce_while`): Roc has `fold_until`;
  keep new collections consistent with it.
- **Grapheme-first text** (Elixir): length, slicing and padding count grapheme
  clusters by default; byte and code point views get explicit names.
- **Transforms separate from sources** (Clojure transducers, Elixir Stream):
  builtin `Iter` is the place; new collections should produce and consume
  `Iter` rather than each defining its own `map`/`keep_if`.
- **Nested-path get and update** (Clojure `get_in`/`update_in`, Elixir
  `Access`): Roc's record update only reaches one level.
- **Structural diff returning three parts** (Clojure `data/diff`: only-in-a,
  only-in-b, in-both): a good shape for Dict and Set diffs.
- **Monotonic time distinct from wall time** (Rust `Instant` vs `SystemTime`):
  trantor's `clocks` already splits them; surface an `Instant`/elapsed type.
- **Codec formats plug into `Encoding`** (Elixir `JSON.Encoder`): CSV and TOML
  should be `Encoding` formats like the builtin `Json`, not separate
  reflection-free parsers.

## Open questions for the grill

- Which gaps belong upstream in the builtins (float `ln`/`exp`, code point
  iteration) versus in trantor packages?
- Package granularity: one `trantor-text`, `trantor-collections`,
  `trantor-encoding` each, or many small packages?
- Pure packages have no host half. Does trantor's package model need anything
  new for interface-free packages, or do they already work as plain components?
- Where does regex live, given its compile-time pattern folding?

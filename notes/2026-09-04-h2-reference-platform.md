# H2 — hand-composed reference platform

Gate H2 of [`plans/2026-09-04-trantor-v1.md`](../plans/2026-09-04-trantor-v1.md).
Fixture: `tests/golden/two-component/`. The golden output H3 must regenerate
byte-for-byte. Hand-written, no tool.

## What it composes

A `reader-cli` world: `./bin/reader <file>` prints the file's line count.

| component | kind | role |
| --- | --- | --- |
| `stdio` | host | exports `stdio` — `stdout_line` |
| `capstdfs` | host | exports `filesystem` — the primitive fs (std::fs stands in for cap-std) |
| `audit` | host | **interposer**: imports fs, exports fs — logs then delegates (D2/D6) |
| `env` | host | exports `env` — `path_arg` (first CLI arg) |
| `pathlib` | **pure Roc** | imports fs, exports `Path` — `read_to_string!`, derived `line_count!`; **no host archive** |
| `cli` | driver | owns the six runtime symbols, `main()`, `roc_main` import (H0a/H0c: one runtime provider) |

Wiring: `stdio = stdio`, `fs = audit(capstdfs)`, `env = env`. Five host
archives + the pure-Roc layer link into one platform; a stock `roc build`
consumes it.

## Verified behavior

- `./bin/reader platform/main.roc` → `[audit] file_read platform/main.roc` (the
  interposer), then `32 lines in platform/main.roc`. `wc -l` agrees: **32**. So
  the full chain runs — app → `Path.line_count!` (pure Roc) → `Fs.file_read!`
  (binding) → `audit` (logs) → `capstdfs` (std::fs) → back, then `Stdio.line!`.
- Missing file → interposer logs, `capstdfs` returns `NotFound`, `Path`
  propagates, app's `?? 0` yields `0`. No crash.
- `build.sh` reproduces `bin/reader` from clean sources (Path.roc copy, glue,
  cargo, stage, `roc check`, `roc build`) — it is the runnable spec for H3.

## Everything the spikes proved, exercised together

Two+ archives (H0a) · hosted union across archives (H0e) · one runtime provider
(H0c) · every boundary `extern "C-unwind"` (H0d) · interposer DAG (D6) · pure-Roc
derived layer with no host symbols (D2) · binding modules per wiring point (D13)
· one shared ABI crate (D4) · `roc check` before `roc build` (H1b) · driver
`requires` with `[requires.uses]` (D18-C).

## Two findings that change H3

### 1. Inter-component host calls resolve at the final link, NOT via cargo deps

First cut gave `audit` a cargo dependency on `capstdfs`. That **bundles
capstdfs's object into `libaudit.a`**, so `trantor__capstdfs__file_read` is
then defined in *two* archives — the H0c duplicate-symbol footgun, reintroduced
by a dependency edge. Fix: `audit` declares `trantor__capstdfs__file_read` as
an `extern "C-unwind"` import and has **no** cargo dep; the symbol resolves when
`roc` links the two archives. Verified by `nm`: the symbol is in `libcapstdfs.a`
only. **Rule for trantor: never emit a cargo dependency between two component
crates for a cross-component host call — emit an extern declaration and let the
final roc link resolve it across archives.** Cargo deps are only for a
component's *own* private crates.

### 2. This glue version cannot build a `RocList<RocStr>` from the host

The plan's driver signature was `main! : List(OsStr) => …`. But the pinned glue
(`release-fast-43746ac5`) gives `RocListWith` only `empty()`/`len()` — no
`push`, no list-of-refcounted constructor. Building `List(OsStr)`/`List(Str)`
from argv by hand means allocating the refcount header and element RocStrs
manually — precisely the "silently corrupt when the layout is wrong" class
(H0c, D3a). The upstream glue redesign explicitly adds these list helpers ("List
helpers that know element width, alignment, and whether element teardown is
required"). Rather than ship hand-rolled refcount code in the *golden
reference*, the fixture passes the path through an `env` interface as a single
`RocStr` (`Env.path_arg! : {} => Str`) and the driver requires `main! : {} =>
Try({}, [Exit(I32), ..])`.

This is a documented deviation from the plan's exact driver signature, forced by
the toolchain, not by trantor. It does not touch H2's intent (multi-component +
interposer + pure-Roc + driver, hand-composed and running) and it actually made
the fixture richer (three host interfaces). **The `List(OsStr)` driver returns
in H5/H7 once the glue redesign lands the list constructors** — tracked as an
open item, and a concrete instance of R4 (the D15 host-signature gap) plus the
plan's stated upstream-glue dependency.

## D7/D16 observation, in passing

`Fs.file_read!` returns `Try(Str, [FileErr(IOErr)])`, `Stdio.line!` returns
`Try({}, [StdioErr(IOErr)])`. Both `IOErr`s come from `roc:cli/io`, but glue
emits **two distinct Rust types** — `FsIOErr` and `StdioIOErr` — because it
names the inner type by the wrapper path it is reached through. Structurally
identical, so the host just constructs each. Worth remembering for H5: a single
userland `IOErr` nominal still yields per-reach Rust twins in glue output; that
is fine (they never need to unify on the host side) but it means "one IOErr" is
a Roc-level guarantee, not a Rust-level one. (Single-tag wrappers like
`[FileErr(IOErr)]` are unwrapped by the compiler, so the err payload is `IOErr`
directly — `FsIOErr` has `NotFound`/`Other`, not a `FileErr` layer.)

## Exit ✅

App runs; fixture checked in under `tests/golden/two-component/`; `build.sh`
reproduces it. H3's acceptance test is defined: regenerate `platform/*.roc`
(binding modules + spliced `main.roc`), the workspace `Cargo.toml`, `abi/`, and
`cli`'s `main()` from `world.toml` + component sources, and diff against the
committed fixture.

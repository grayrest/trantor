# H4 — Roc shims, cell, Tier 1

Gate H4 of [`plans/2026-09-04-hematite-v1.md`](../plans/2026-09-04-hematite-v1.md).
Fixture: `tests/golden/roc-shim/` (+ a Tier-1 demo over the H2 platform).

## H4a/b — Roc shim fulfills fs, cell holds state ✅

`memfs` is a `kind = "roc"` component that **exports `filesystem`** — the same
interface `capstdfs` implements in Rust. It has no host archive. It ships the
`Fs` binding module *with bodies* (D13/D19): `file_read!` matches an in-memory
table. The world wires `filesystem = memfs` instead of `audit(capstdfs)`.

- `reader hello.txt` → `3 lines`, `one.txt` → `1 lines`, missing → `0 lines`.
- **`Path.roc` is byte-identical** to the two-component fixture (asserted in
  `verify.sh` with `diff`). The derived layer does not know or care whether fs
  is host or Roc — the substitution proof (D2/D19).
- **cell round-trip is an assertion inside the read.** `file_read!` does
  `Cell.put!(path)` then `Cell.get!({})` and errors unless they match, so a
  successful `3 lines` also proves host-backed mutable state reached from a Roc
  shim (D12). `cell` is a minimal host component (one process-global `Mutex<String>`
  slot; a real cell is typed and handle-based).

### Codegen change

The tool now distinguishes host- vs Roc-implemented wiring points
(`resolve::roc_impls`). For a Roc impl: the binding module is copied from the
**shim component** (it ships the module with bodies), and **no `hosted{}` entry**
is emitted (a Roc shim forwards, it does not cross to the host). For a host impl:
unchanged from H3 (declaration module from the interface + a mangled `hosted{}`
entry). H3's `verify.sh` still passes — the host path is untouched.

## H4c — Tier 1: pure-Roc extension, no Rust toolchain ✅

Took the *prebuilt* two-component platform (archives + glue already produced,
copied to a temp "consumer" location) and added a pure-Roc module `Greet` (zero
host symbols). Built a consumer app with **only `roc build`** — no `cargo`, no
`roc glue`. It linked against the prebuilt `libstdio.a`/etc. (mtimes predate the
app build by 23s) and printed `=== tier one ===`.

Confirms D11: a component that adds no hosted symbols leaves glue output and the
prebuilt `libhost` archives valid, so extension needs no Rust toolchain.

**Finding for hematite's Tier-1 path:** a Tier-1 add requires **two `main.roc`
edits** — add the module to `exposes` *and* add an `import <Module>` line. An
exposed-but-unimported module is `exposed but not defined` under `roc check` and
a **segfault** under `roc build` (same class as H1b). hematite must emit both
edits and must `roc check` before `roc build`.

## Notes carried forward

- The `exposes ⇒ must-import` rule is now a second instance (with H1b) of
  "get the `exposes` list wrong → `roc build` segfault, `roc check` diagnoses."
  The `roc check`-before-`build` gate covers both.
- `cell` being in the baseline is what keeps a stateful Roc shim in Tier 1: the
  shim adds no host symbols of its own; the state lives in the baseline's `cell`
  host component (D12). If `cell` weren't already published, adding a stateful
  shim would be a Tier-2 (host) change.

## Exit ✅

`tests/golden/roc-shim/verify.sh`: regenerates byte-for-byte, asserts `Path.roc`
identical across wirings, builds, and runs (`3 lines`, which also passes the
cell round-trip). Tier-1 demonstrated over the H2 platform with the Rust
toolchain unused.

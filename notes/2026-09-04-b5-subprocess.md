# B5 — `roc:subprocess` (seahaven's design)

Gate B5 of [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md).
Fixture: `tests/golden/b5-subprocess/` (`verify.sh`). Roc-native, no WASI
precedent (P3); seahaven's 13-function `Cmd` rather than basic-cli's 5.

## Result ✅ (host compiled first try)

`output: hello from a child` (a PATH-searched `echo` with captured stdout),
`exit-code: 7` (`sh -c "exit 7"`), `path-search: ok` (`check_available!("ls")`
running seahaven's full PATH-search logic), exit 0.

**seahaven's 499-line `Cmd.roc` ports with exactly two changed lines** (the
`Env.var!` call sites, bridged by one appended helper) and its 247-line
`OsStr.roc` ships **byte-verbatim** (`verify.sh` `cmp`s it and bounds the
`Cmd.roc` diff at 2 removed lines). Verbatim/near-verbatim count: 12 modules.

## The one design call: keep seahaven's crossing shape

`Cmd.roc` references `Host.Cmd` as a type, maps `program`/`args`/`envs` through
`OsStr.to_raw` into it, and its PATH-split *matches on `to_raw`'s three
variants*. So the `roc:subprocess` crossing record keeps seahaven's
`NativeOsStr = [Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]` rather
than a `Str`. That does not contradict P11: P11 is about the *path packages*;
this is a subprocess argument crossing, and keeping seahaven's union is what
makes the seam verbatim. The host reads `Utf8` (all Roc ever mints here) and
honors `UnixBytes` raw. Glue emits the union as `UnixBytesOrUtf8OrWindowsU16s`
with `RocList<…>` (refcounted elements) for `args`/`envs`.

## Bridges and additions

- **`env_var_os!`** (appended to `Cmd.roc`): seahaven's `Env.var!` is
  OsStr-typed, this platform's is Str-typed. The helper re-wraps `Ok`/`Err` into
  seahaven's exact types; any other error is treated as not-found (an open-row
  catch-all can't unify across the two payload types). R8: migration ≠
  composition — two call-site edits, nothing structural.
- **`Path`/`OsPath` gained** `utf8`, `unix_bytes`, `windows_u16s` (seahaven's
  constructors — Str-backed here, lossy for non-UTF-8 bytes on `roc:path`,
  `Raw` on `roc:os-path`), `is_executable!` (over `FsOps.executable!` from the
  B3 `stat_at!` record), and `is_eq`/`to_hash` — because seahaven's own
  `expect`s compare `List(Path)` with `==`. Same surface on both packages (P11).
- **`Host` shim** declares seahaven's `Cmd`/`CmdOutputSuccess`/`CmdOutputFailure`
  types and forwards the four leaves to `SubprocessHost`.

## Host notes

- The four `*Args` structs glue emits are layout-identical; one `command()`
  builder consumes them via a `transmute_copy` view. `envs` arrive flattened
  `[k1, v1, k2, v2, …]` (seahaven's `flatten_arg_pairs`) and are re-paired.
- Owned-argument rule (B0): the whole `Args` struct is owned; `args`/`envs`
  (lists) and `program` (the union) are `.decref`'d after conversion.
  **Open:** whether `RocListWith<T, true>::decref` releases the *elements* or
  only the spine (the glue also offers `release_with<P>`); a leak here would be
  per-call element strings, not a crash. Worth a gauge in B8.
- `exec_status!` reports a signal-killed child as the negative signal number
  (`ExitStatusExt::signal`), matching seahaven's contract.

## Findings

1. **`check_available!` returns `Bool`**, not `Try` — an `if`, not a `match`.
2. Seahaven's `expect`s exercise `Path.unix_bytes([0x2F, 0xFF, 0x62])` (a
   non-UTF-8 byte) and compare with `==`; on the lossy `roc:path` both sides
   pass through the same constructor so equality holds. Only typechecked here
   (`roc check`), not run under `roc test`.

## Exit ✅

`verify.sh`: `OsStr.roc` verbatim, `Cmd.roc` bounded at 2 changed lines,
compose/build/run, exact three-line stdout, exit 0.

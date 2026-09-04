# B3 — `roc:filesystem` + `roc:path` + `roc:os-path` (the capability model)

Gate B3 of [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md).
Fixture: `tests/golden/b3-fs/` (`verify.sh`). Implements P4/P8/P11/P14 over
B0's resources and B1's streams.

## Result ✅ — three worlds, one app, unchanged

A basic-cli-shaped file program (`Path.create_all!` → `write_utf8!` →
`read_utf8!` → `is_file!` → `list!` → `Env.cwd!` → a capability probe →
`delete!` → `delete_empty!`) runs on three compositions with **no app edits**:

| world | fs component | `Path` is | `escape:` probe (`/etc/hosts`) | exit (= live `Descriptor`s) |
| --- | --- | --- | --- | --- |
| unconfined | `fs-unconfined` (preopen `/`) | `[Path(Str)]` | **escaped** (readable — basic-cli parity) | 0 |
| confined | `fs-confined` (preopen = cwd) | `[Path(Str)]` | **denied** — capability error | 0 |
| ospath | `fs-unconfined` | `[Text(Str), Raw(List(U8))]` | escaped | 0 |

The host compiled first try. Every op mints and drops descriptor resources;
`live=0` at exit in all three worlds (B0's drop-balance under ~15 mints/run).

## What is proven

- **P4 — the capability model.** Primitives are WASI-shaped and descriptor-
  relative: `preopen_at!`, `open_at!`, `read_via_stream!`, `read_file_at!`,
  `write_file_at!`, `stat_at!`, `read_dir_at!`, `create/remove_dir[_all]_at!`,
  `unlink_at!`, `rename_at!`, `link_at!`. No ambient path authority at this
  layer. basic-cli's path API is derived sugar (`FsOps` resolves against
  preopen 0).
- **P14 — confinement is a swappable component, not a fork.** `fs-unconfined`
  and `fs-confined` are two thin staticlibs over one `fs-core` rlib; the only
  difference is `Root { base, confined }`. The confined root canonicalizes the
  deepest existing ancestor and refuses any resolution outside `base`
  (`PermissionDenied`). The world wires one; the app never knows. This is
  seahaven's cap-std confinement re-expressed as a wiring choice.
- **P11 — bytes primitive, two path packages, world-rename swap.** Paths cross
  as `List(U8)`. `roc:path` (Str; non-UTF-8 listing entries *excluded*,
  Gleam/WASI model) and `roc:os-path` (lossless `[Text|Raw]`; kept as `Raw`)
  share every function name and the `FsOps` bytes core. A world exposes
  `"OsPath as Path"` and the app compiles unchanged against the lossless impl.
- **P8 — cwd is a userland `cell` prefix.** `FsOps.cwd!` = the `cell` value if
  set, else the process cwd; `Env.set_cwd!` just `Cell.put!`s. Relative paths
  resolve under it; absolute ones pass through to the (possibly confining)
  root. `Env.cwd!`/`set_cwd!`/`exe_path!`/`temp_dir!` restored, Str-based.
- **D14 world-rename is now a tool feature.** `exports = ["OsPath as Path"]` on
  a `kind = "roc"` component copies the module with its identifier substituted
  (word-boundary `rename_ident`). Roc has no re-export, so this is how "both
  path packages ship, the world picks" is operationally real.

## Structure

- `fs-core` (rlib, **no `no_mangle`**): `Root`, `Desc` resource payload
  (`Dir(PathBuf) | File(File)`), `resolve` + `canon_lenient`, every op body,
  the two `IOErr` twin constructors, and an `exports!(prefix, root)` macro that
  a thin staticlib invokes to emit its 16 hosted symbols via `paste`
  (`#[export_name = concat!(…)]` isn't allowed, so ident-paste it is). The
  no_mangle symbols live only in the staticlibs (H0c).
- `FsOps` (pure Roc): NUL-joined `read_dir_at!` split into `List(List(U8))`
  in Roc (R-B5 dodge, filenames never contain NUL); one `stat_at!` record
  serves basic-cli's eight `is_*`/`size`/`time` leaves.

## Findings

1. **BSD `sed` has no `\b`.** Deriving `OsPath.roc` from `Path.roc` with
   `sed 's/\bPath\b/OsPath/g'` was a silent no-op, and the follow-up patch
   (matching as-if-renamed text) silently matched nothing — so the rename world
   quietly shipped the Str impl under the name `Path` and *typechecked*, because
   the surfaces are identical by design. Caught only by asserting the
   declaration line in `verify.sh`. Use `perl -pe` for `\b`; and the fixture
   now asserts zero stray `Path` tokens in `OsPath.roc`. Lesson for the port:
   a swap whose two sides are name-compatible needs a *content* assertion, not
   a typecheck.
2. **Uppercase-leading helper names are tags.** `FsOps_join` parsed as a tag
   constructor (`[FsOps_join(..), ..]` in the type error), not a function.
   Module-level helpers must be lowercase. Char literals also don't work in
   pattern position (`Ok('/') =>`); compare in a guard instead.
3. `cp -r src dest` into a pre-created `dest` nests (`cell/cell/`) — the same
   trap as H5's capture-stdio. Third time; noted so it stops.

## Deferred from B3 (plan-visible)

- `File.Reader` (basic-cli's streaming `read_line!`) — needs Roc-side line
  buffering over an `InputStream` (a leftover-bytes `cell`); the `open_at!` +
  `read_via_stream!` primitives it needs exist and are exercised by the
  `read_file_at!` path, but the derived `Reader` is not yet written.
- Times are `u64` ns (`modified_ns`); basic-cli's `U128` `time_*` and
  `accessed`/`created` are not surfaced yet.
- The root descriptor is minted per op (`Fs.preopen_at!(0)` each call); a
  cached root would need a typed cell holding a `Box` — later.

## Exit ✅

`verify.sh`: asserts the lossless `OsPath.roc` content, then composes/builds/
runs all three worlds, checking each world's `platform/Path.roc` declaration,
the full stdout, the `escape:` verdict, and exit 0 (drop-balance).

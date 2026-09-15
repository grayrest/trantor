# S2 — trantor-cli host gaps, trantor-files, trantor-process

**Design log:** `notes/2026-09-14-s2-cli-host-gaps-design-log.md`, D-S2-1 to
D-S2-21 (D-S2-18..21 reorganise the earlier ones across packages). Repos:
`~/dev/roc/trantor-cli`, new `~/dev/roc/trantor-files`, new
`~/dev/roc/trantor-process`. Gate at every commit: `trantor test .` in each
touched package, clippy `-D warnings` on host crates, no warnings.

## Why

D-S1-11 step 2. trantor-cli cannot append, stream a write, walk or glob, create
temp files, copy with permissions, or pipe to and from a child; rocjust fakes a
pipe with `/tmp` files and `sh -c`. And a baseline that can spawn is a
confinement escape (D-S2-19), so process creation leaves it.

## Package layout

| Package | Gains | Loses |
|---|---|---|
| trantor-cli | `Fs` primitives, `FdHandoff`, path-type methods, `File.Writer`, `Reader/Writer.descriptor` | `Subprocess`, `Cmd`, `subprocess-host`, `host-shim`'s `cmd_exec_*`, 3 test suites |
| trantor-files (new, pure Roc, `[deps] trantor-cli`) | `Walk`, `Glob`, `Temp`, `Tree` | — |
| trantor-process (new, `[deps] trantor-cli`) | `Subprocess`, `Cmd`, `subprocess-host`, the 3 suites, `tests/spawn` | — |

## trantor-cli

`interfaces/fs/Fs.roc`:

```roc
open_at! : Descriptor, List(U8), {
    read ?: Bool, write ?: Bool, create ?: Bool, exclusive ?: Bool,
    truncate ?: Bool, directory ?: Bool, follow_symlinks ?: Bool,
} => Try(Descriptor, [Io(IOErr)])          # Roc body: defaults + validation (D-S2-3)
open_with_flags_at! : Descriptor, List(U8), { read : Bool, write : Bool, create : Bool, exclusive : Bool, truncate : Bool, directory : Bool, follow_symlinks : Bool } => Try(Descriptor, [Io(IOErr)])   # hosted
write_via_stream!  : Descriptor, U64 => Try(Streams.OutputStream, [Io(IOErr)])
append_via_stream! : Descriptor => Try(Streams.OutputStream, [Io(IOErr)])
read_dir_at!  : Descriptor, List(U8) => Try(List({ name : List(U8), kind : [File, Dir, SymLink, Other] }), [Io(IOErr)])
copy_file_at! : Descriptor, List(U8), Descriptor, List(U8) => Try({}, [Io(IOErr)])
readlink_at!  : Descriptor, List(U8) => Try(List(U8), [Io(IOErr)])
symlink_at!   : Descriptor, List(U8), List(U8) => Try({}, [Io(IOErr)])   # target, link
```

[ASSUMPTION: `open_with_flags_at!` stays visible in `Fs`, documented as the
wrapper's target; hide it if trantor's interface model allows a hosted leaf apps
cannot call.]

New `interfaces/fd-handoff/FdHandoff.roc`, not in package `exports`
(D-S2-14, D-S2-21):

```roc
descriptor_fd! : Fs.Descriptor => Try(I32, [NotAFile])
output_fd!     : Streams.OutputStream => Try(I32, [NotAFile])
input_fd!      : Streams.InputStream => Try(I32, [NotAFile])
```

Implemented by the fs components (descriptor) and `sync-io` (streams);
`sync-io-core`'s `Input`/`Output` record an optional raw fd at mint time.

Derived (D-S2-5, D-S2-20, D-S2-21): `FsOps` `open!`, `append!`,
`open_writer!`, `open_append!`, `copy!`, `sym_link!`, `read_sym_link!`,
`list!` over entries. Methods on `Path` (basic-cli, through new `Host`
filesystem leaves), `OsPath`, `StrPath`: `append_bytes!`, `append_utf8!`
(`OsPath`: bytes only), `copy!`, `sym_link!`, `read_sym_link!`, existing
`hard_link!`. `File.Writer` (`open_writer!`, `open_append!`, `write!`,
`write_utf8!`, `line!`, `descriptor`); `File.Reader.descriptor`.

Host (`fs-core`): `open_with_flags_at` via `std::fs::OpenOptions` /
`cap_std::fs::OpenOptions` (`O_NOFOLLOW` for `follow_symlinks: False`);
write/append streams minted with their fd; `read_dir_at` with `file_type()`
not following links; `copy_file_at` (`std::fs::copy` / `cap_std::fs::Dir::copy`);
`readlink_at`, `symlink_at` via cap-std. Every op through `resolve`.

README: the process layer moved to `trantor-process`; a world without it cannot
start processes; a `Cmd` user adds one `[deps]` line.

## trantor-files

`package.toml` add-on, `[deps] trantor-cli`, components:

- `glob` (pure): `Glob.matches : Str, Str -> Bool`, pattern parsing with
  `InvalidGlob(Str)`, literal prefix (D-S2-9). Expect corpus.
- `files-lib`: over trantor-cli's `Path` and `FsOps`/`Fs`.
  - `Walk.walk! : Path, state, { follow_symlinks ?: Bool, max_depth ?: U64 },
    (state, Entry => [Continue(state), SkipDir(state), Stop(state)]) =>
    Try(state, [PathErr(IOErr), ..])`, `Walk.list!` (D-S2-8).
  - `Glob.expand! : Str => Try(List(Path), [PathErr(IOErr), InvalidGlob(Str), ..])`.
  - `Temp.with_dir!`, `Temp.with_file!` (`Path, File.Writer`),
    `Temp.create_dir!`, `Temp.create_file!`, each with `_in` (D-S2-6, -7);
    names from `Random.seed_u64!`, exclusive create, retry on `AlreadyExists`.
  - `Tree.copy! : Path, Path, { overwrite ?: Bool } => …` — walk +
    `Path.copy!` + `create_dir!`, links through `read_sym_link!`/`sym_link!`
    (D-S2-10).

[ASSUMPTION: `Glob.matches` and `Glob.expand!` share one `Glob` module in
`files-lib` importing the pure matcher component; if a pure component and an
effectful module cannot share the name, the pure one is `GlobPattern`.]

## trantor-process

`package.toml` add-on, `[deps] trantor-cli`; `subprocess-host` imports
`fd-handoff` and reaches the cwd through `cwd-host`'s extern as today.

`interfaces/subprocess/Subprocess.roc` (D-S2-11, -12):

```roc
Stdio : [Inherit, Null, Pipe, Descriptor(Fs.Descriptor), ToStream(Streams.OutputStream)]
ExitStatus : [Exited(I32), Signaled(I32)]
Child :: Box(U64)
spawn! : Cmd, { stdin ?: Stdio, stdout ?: Stdio, stderr ?: Stdio } => Try(Child, [Io(IOErr)])   # Roc body over a hosted leaf taking fds
Child.pid, stdin!, stdout!, stderr!, close_stdin!, wait!, try_wait!, signal!, collect!
```

`cmd-lib/Cmd.roc`: the basic-cli functions over `Subprocess` (D-S2-13 table and
rules), `Cmd.spawn!` (D-S2-15). Host: today's `command()` builder; `spawn` with
`dup`ed fds; `Child` resource; `collect` with stdin writer + two reader threads,
read to EOF, then wait; `try_wait`; `kill` via `libc::kill`.

## Work — commit at each

trantor-cli:

1. **Open flags.** Hosted leaf, Roc wrapper, `FsOps.open_read!` migrated;
   expects for defaults and rejections.
2. **Write streams and `File.Writer`.** `write_via_stream!`,
   `append_via_stream!`, `append_*` and `Writer` on all path types.
   `tests/fs-write-modes`.
3. **Directory entries.** `read_dir_at!` with kinds; `FsOps.list!` and path
   types migrated; existing suites unchanged.
4. **Links and single-file copy.** `copy_file_at!`, `readlink_at!`,
   `symlink_at!`; `copy!`, `sym_link!`, `read_sym_link!`. `tests/links`
   (exec bit preserved, existing-destination error, link round trip).
5. **Confinement.** `tests/confined-race` over every new op, including a
   planted `symlink_at!` link.
6. **`FdHandoff`** and `Reader/Writer.descriptor`; fds recorded on streams.
   Expects that a file stream answers and a test backing returns `NotAFile`.

trantor-process:

7. **Move.** New repo; `Subprocess` (still the four `exec_*`), `Cmd`,
   `subprocess-host` and the three suites move unchanged; trantor-cli drops them
   and `host-shim`'s `cmd_exec_*`, with its `exports`, `[provides]`, README and
   `tests/surfaces`/`consumer` updated. Both packages' `trantor test .` pass.
8. **Spawn.** `Stdio`, `Child`, `spawn!`, `collect!` and the rest.
   `tests/spawn` (>64 KB streamed; both outputs piped through `collect!`;
   `Descriptor` and `ToStream` redirects; `close_stdin!`; `try_wait!`;
   `signal!` → `Signaled`).
9. **exec over spawn.** Remove the four `exec_*` leaves; `Cmd` per D-S2-13;
   `Cmd.spawn!`. The three suites unchanged; expects for the `ExitStatus`
   mappings.

trantor-files:

10. **Glob matcher** with the corpus.
11. **Walk and `Glob.expand!`.** `tests/walk-glob`.
12. **Temp.** `tests/temp`.
13. **Tree copy.** `tests/copy`.

Docs:

14. READMEs for all three; D-S1-11 step 2 pointer to the three packages;
    implementation notes here.

## Implementation notes

(Filled in during work.)

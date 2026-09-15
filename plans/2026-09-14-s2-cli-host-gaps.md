# S2 — trantor-cli host gaps

**Design log:** `notes/2026-09-14-s2-cli-host-gaps-design-log.md`, D-S2-1 to
D-S2-17. Repo: `~/dev/roc/trantor-cli` (branch per the feature workflow).
Gate at every commit: `trantor test .` in trantor-cli, clippy `-D warnings` on
the host crates, no warnings.

## Why

D-S1-11 step 2. trantor-cli cannot append, stream a write, create a temp file,
walk or glob a tree, copy with permissions, or pipe to and from a child.
rocjust fakes a pipe with `/tmp` files and `sh -c`.

## Surface

### Raw layer

`interfaces/fs/Fs.roc`:

```roc
open_at! : Descriptor, List(U8), {
    read ?: Bool, write ?: Bool, create ?: Bool, exclusive ?: Bool,
    truncate ?: Bool, directory ?: Bool, follow_symlinks ?: Bool,
} => Try(Descriptor, [Io(IOErr)])          # Roc body: defaults + validation (D-S2-3)
open_with_flags_at! : Descriptor, List(U8), { read : Bool, write : Bool, create : Bool, exclusive : Bool, truncate : Bool, directory : Bool, follow_symlinks : Bool } => Try(Descriptor, [Io(IOErr)])   # hosted
write_via_stream!  : Descriptor, U64 => Try(Streams.OutputStream, [Io(IOErr)])
append_via_stream! : Descriptor => Try(Streams.OutputStream, [Io(IOErr)])
read_dir_at!  : Descriptor, List(U8) => Try(List({ name : List(U8), kind : [File, Dir, SymLink, Other] }), [Io(IOErr)])   # replaces NUL-joined
copy_file_at! : Descriptor, List(U8), Descriptor, List(U8) => Try({}, [Io(IOErr)])
readlink_at!  : Descriptor, List(U8) => Try(List(U8), [Io(IOErr)])
symlink_at!   : Descriptor, List(U8), List(U8) => Try({}, [Io(IOErr)])   # target, link
```

[ASSUMPTION: the hosted leaf's name `open_with_flags_at!` is exported in the
module but documented as the wrapper's target; if trantor's interface model
allows a hosted leaf without app visibility, hide it instead.]

`interfaces/subprocess/Subprocess.roc` (the four `exec_*` removed):

```roc
Stdio : [Inherit, Null, Pipe, Descriptor(Fs.Descriptor), ToStream(Streams.OutputStream)]
ExitStatus : [Exited(I32), Signaled(I32)]
Child :: Box(U64)
spawn! : Cmd, { stdin ?: Stdio, stdout ?: Stdio, stderr ?: Stdio } => Try(Child, [Io(IOErr)])   # Roc body over a hosted leaf taking fds
Child.pid, stdin!, stdout!, stderr!, close_stdin!, wait!, try_wait!, signal!, collect!   # D-S2-12
```

New unexported `interfaces/fd-handoff/FdHandoff.roc` (D-S2-14):

```roc
descriptor_fd! : Fs.Descriptor => Try(I32, [NotAFile])
stream_fd!     : Streams.OutputStream => Try(I32, [NotAFile])
input_fd!      : Streams.InputStream => Try(I32, [NotAFile])
```

Implemented by the fs components (descriptor) and `sync-io` (streams).
`sync-io-core`'s `Input`/`Output` gain an optional raw fd recorded at mint time.

### Derived

- New pure component `glob`: `Glob.matches`, `Glob.literal_prefix`, pattern
  parsing with `InvalidGlob(Str)` (D-S2-9).
- `fs-ops/FsOps.roc`: `open!` with flags, `append!`, `open_writer!`,
  `open_append!`, `walk!`, `copy!`, `copy_all!`, `sym_link!`,
  `read_sym_link!`, `create_temp_dir!`, `create_temp_file!` (+ `_in`),
  `with_temp_dir!`, `with_temp_file!`, `glob!`.
- `basic-cli-lib`: `Path` and `File` additions (D-S2-5, -6, -8, -9, -10,
  -16), `File.Writer`, `Reader.stdio`, `Writer.stdio`, through new `Host`
  filesystem leaves over `FsOps` (the filesystem `Host` seam stays; only the
  subprocess adapters go).
- `os-path/OsPath.roc`, `path/Path.roc` (StrPath): the same operations.
- `cmd-lib/Cmd.roc`: `exec_*` over `Subprocess` directly (D-S2-13 table and
  rules); `Cmd.spawn!` (D-S2-15). `host-shim/Host.roc` loses `cmd_exec_*`.

### Host

- `fs-core`: `open_with_flags_at` through `std::fs::OpenOptions` /
  `cap_std::fs::OpenOptions` (`custom_flags(O_NOFOLLOW)` for
  `follow_symlinks: False`); write/append streams minted with their fd;
  `read_dir_at` returning entries with `file_type()` (not following links);
  `copy_file_at` (`std::fs::copy` / `cap_std::fs::Dir::copy`); `readlink_at`,
  `symlink_at` (cap-std `read_link`/`symlink`). Every op through `resolve`.
- `subprocess-host`: one `Command` builder (today's `command()`), `spawn`
  with `Stdio::from(OwnedFd::from(dup(fd)))` for descriptor/stream stdio;
  `Child` resource; `collect` with two reader threads + a stdin writer thread,
  read to EOF, then wait; `try_wait`, `kill` via `libc::kill`.

## Work — commit at each

1. **Open flags.** Hosted `open_with_flags_at!`, Roc `open_at!` wrapper with
   defaults and validation, `FsOps.open_read!` migrated. Unit expects for
   defaults and each rejected combination.
2. **Write streams.** `write_via_stream!`, `append_via_stream!`;
   `Path.append_*`, `File.Writer`, `OsPath`/`StrPath` equivalents.
   `tests/fs-write-modes`.
3. **Directory entries.** `read_dir_at!` returns kinds; `FsOps.list!` and
   the path packages migrated; existing suites unchanged.
4. **Walk.** `walk!`, `walk_list!` in `FsOps` and the three path modules.
5. **Glob.** `glob` component with the expect corpus; `glob!`.
   `tests/walk-glob` (hidden entries, symlink loop, unreadable dir, order,
   `SkipDir`/`Stop`, `max_depth`, literal prefix).
6. **Temp.** Scoped and manual, `_in` variants, retry on `AlreadyExists`.
   `tests/temp`.
7. **Links and copy.** `readlink_at!`, `symlink_at!`, `copy_file_at!`;
   `sym_link!`, `read_sym_link!`, `copy!`, `copy_all!`. `tests/copy`.
8. **Confinement.** Extend `tests/confined-race` to every new filesystem op
   under the concurrent swapper, including a planted `symlink_at!` link that a
   later read must not follow outside.
9. **Spawn.** `FdHandoff`, `Stdio`, `Child`, `spawn!`, `collect!` and the rest
   in `Subprocess` and `subprocess-host`. `tests/spawn` (>64 KB streamed;
   both outputs piped through `collect!`; `Descriptor` and `ToStream`
   redirects; `close_stdin!`; `try_wait!`; `signal!` → `Signaled`).
10. **exec over spawn.** Remove the four `exec_*` leaves and `Host`'s
    `cmd_exec_*`; `Cmd` rebuilt per D-S2-13; `Cmd.spawn!`, `Reader.stdio`,
    `Writer.stdio`. `three-defects`, `relative-cwd`, `child-signal-mask`
    unchanged; unit expects for the `ExitStatus` mappings.
11. **README.** New operations; the child-is-not-confined statement; the
    two-pipe deadlock rule; trantor plan implementation notes.

## Implementation notes

(Filled in during work.)

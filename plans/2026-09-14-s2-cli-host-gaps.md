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

### trantor-cli (steps 1–6: `bdc5f2e`..`1c038ac`)

- **Gate.** `trantor test .` in the package; clippy on the composed world's
  crates only: `cargo clippy --release --no-deps -p fs-core -p sync-io -p …
  -- -D warnings` inside `target/trantor/<world>` (a plain workspace clippy
  lints the generated `trantor-abi` and fails there). `subprocess-host` fails
  `wrong_self_convention` at `1c51dc7` already; it is rewritten in step 9.
- **A hosted leaf must be declared above a Roc body that calls it** in the same
  interface module, or the body reports the leaf as not existing. `Fs.roc`
  lists `open_with_flags_at!` before `open_at!`; `FdHandoff.roc` its leaves
  before `descriptor_fd!`.
- **`open_with_flags_at!` stays visible** in `Fs` (the plan's assumption): an
  exported interface module has no per-leaf hiding.
- **`IOErr` has no equality**, so the open-flag expects compare through a
  `settled` helper that maps the refusal to a plain tag.
- **Writers open with `read: False`**, or a write-only file is refused.
  `write_via_stream!` writes with `pwrite` at its own offset;
  `append_via_stream!` sets `O_APPEND` on the open file (documented: later
  offset streams on that descriptor append too).
- **`File.Writer` is opaque (`::`)**, so `StrPath`/`OsPath` build one through
  `File.Writer.from_host`. Their new methods keep those modules' closed
  `[FileErr(IOErr)]` unions.
- **`FsOps.entries!`** returns the kinded entries (for trantor-files' walk);
  `FsOps.list!` maps it to names. `FsOps.Reader`/`Writer` records carry the
  descriptor beside the stream; `Host.FileReader` is `FsOps.Reader`.
- **cap-std's `Dir::copy` escapes on macOS.** It tries
  `fclonefileat(src, root_fd, "sub/name")`, which resolves the destination's
  directories itself: 108 of 2000 raced copies landed outside the root. The
  confined copy opens both files through cap-std (`create_new` + mode), copies
  with `std::io::copy` and sets the permissions; unconfined keeps
  `std::fs::copy` after a `symlink_metadata` existence check.
- **D-S2-22 (user, during step 5):** under confinement `symlink_at!` resolves
  the target from the link's directory beneath the root and refuses a missing
  one. `Tree.copy!` (step 13) must create links after their targets.
- **`confined-race`** runs 10 operations × 2000 against the swapper, per-op
  escape count plus both controls, then diffs the outside directory (what the
  writes left) and checks a confined copy kept mode 750.
- **`FdHandoff` is provided by `sync-io` alone.** One interface is wired to one
  component, so the fs components cannot implement `descriptor_fd!` beside
  sync-io's stream leaves. `descriptor_fd!` is a Roc body:
  `input_fd!(Fs.read_via_stream!(d))` — a directory descriptor's stream has no
  fd, so `NotAFile`. `Input`/`Output` gained an `Option<RawFd>` field set at
  mint (files, stdio).
- **Every handed-off fd is an owned `F_DUPFD_CLOEXEC` duplicate**, taken
  inside `resource::with`: the first version duped after the borrow and got a
  closed fd, because a stream minted for the call is dropped on release. Added
  `close_fd! : I32 => {}` so a caller can release an fd it did not hand on
  (trantor-process needs it when a second redirect fails after the first
  succeeded).
- **Step 6 is an integration suite, not expects:** expects cannot make hosted
  calls. `tests/fd-handoff` adds `FdHandoff` to its world's `exports` and reads
  the handed fd back through `/dev/fd/N`.

### trantor-process (steps 7–9: `80444b7`..`5ba0dc7`, docs `283ed10`, `bf40eb5`)

- **New repo `~/dev/roc/trantor-process`, no remote.** `subprocess-host`
  imports `sync-io` and `fd-handoff`; its cwd still comes from `cwd-host`'s
  extern.
- **`three-defects` is in both packages.** Moving it whole would have taken the
  `File.Reader` line cap and non-UTF-8 `Env.var!` checks out of trantor-cli, so
  trantor-cli keeps a copy without the `Cmd` case and trantor-process keeps all
  three. `relative-cwd` and `child-signal-mask` moved as planned.
- **trantor golden `b8-basic-cli`** (`aaec52e`): `world.toml` adds
  trantor-process for basic-cli's `Cmd` examples; `world-confined.toml` does
  not, since a confined world must not. `verify.sh` passes after step 7 and
  again after step 9.
- **`Child` is `:: { handle : Handle, pid : I32 }`**, not `:: Box(U64)`:
  `Child.pid` is pure and a hosted leaf cannot be, so the pid is read once at
  spawn. The resource is `Subprocess.Handle`; the hosted leaves are
  `spawn_redirected!` and `handle_*!`, and `Child`'s methods are Roc bodies.
- **Redirects cross as `[Inherit, Null, Pipe, Fd(I32)]`.** `spawn!` turns
  `Descriptor`/`ToStream` into owned fds through `FdHandoff`; if one has no fd
  it closes the others with `close_fd!` and answers
  `Io(Other("a Descriptor or ToStream redirect has no file descriptor under it"))`.
  The host wraps each fd in `OwnedFd` before anything can fail.
- **Pipes are stream resources minted once at spawn** and held by the handle;
  `stdout!` returns another reference to the same box (refcount increment), so
  a read's buffered bytes survive to the next call. `close_stdin!` swaps the
  writer for one answering `BrokenPipe`. `collect!` lends the three backings to
  scoped threads; a stdin write error is ignored (the status says what the
  child did).
- **`CmdStatus`** (new internal module in `cmd-lib`) holds the three
  `ExitStatus` mappings as pure functions with expects. `Cmd.spawn!` error
  is `SpawnFailed({ command: to_str(cmd), err })`.
- **Glue names `IOErr` twins by the first leaf that reaches them**:
  `spawn_redirected!`'s is `SubprocessIOErr`, the handle leaves' `IOErr`, and
  the names moved when the `exec_*` leaves were removed. `subprocess-host` has
  one constructor per twin.
- **Open error unions on `spawn!` and `Child`** (`283ed10`), found writing
  `tests/readme`: `child.wait!()?` did not compile against an app's open
  union. Hosted leaves stay closed; the Roc bodies reopen.
- **Clippy:** the moved crate's pre-existing `wrong_self_convention` was fixed by
  renaming; the trait went with the `exec_*` calls in step 9.

### trantor-files (steps 10–13: `66e676f`..`299b95a`, docs `08ca5d3`)

- **New repo `~/dev/roc/trantor-files`, no remote.** Components: `glob`
  (`GlobPattern`, `GlobParse`, `GlobText`) and `files-lib` (`Glob`, `Walk`,
  `Temp`, `Tree`, internal `FilesPath`).
- **The pure matcher is `GlobPattern`** (the plan's fallback): two modules in
  one platform cannot share `Glob`. `Glob.matches` delegates to it.
- **Glob semantics settled in code:** `a/**` does not match `a`; `**` next to
  other characters is `InvalidGlob`; nested braces are `InvalidGlob`; `?` and
  classes match a UTF-8 scalar (`GlobText`), malformed bytes as U+FFFD; `**`
  also refuses hidden components. `GlobPattern.could_contain` drives
  `SkipDir` in `expand!`; `literal_prefix` is the common literal leading
  components of all brace alternatives.
- **`Glob.expand!`** checks the start directory first (missing or not a
  directory is `Ok([])`), then any walk error propagates. Paths come back
  without the walk's `./` and sorted by bytes.
- **Walk with `follow_symlinks: True`** reports a link that can be listed as
  `IsDir` (there is no following stat primitive); a link cycle ends in the OS's
  path-too-long error.
- **`Temp`** names `trantor-` + 16 hex digits from `Random.seed_u64!`, 8
  attempts; files open `{ write, exclusive, follow_symlinks: False }` through
  `Fs.open_at!` with `FsOps.root!`/`resolve!`, wrapped by
  `File.Writer.from_host`. Cleanup failing after an `Ok` callback returns the
  cleanup's error. **Permissions follow the umask** (0644/0755 in `/tmp`, where
  `mkstemp` uses 0600): no primitive sets a mode. Raised with the user as a
  follow-up, not changed.
- **`Tree.copy!`** lists the walk first, makes directories and copies files in
  walk order, then links (D-S2-22). A socket/FIFO is `Unsupported`.
  `tests/copy` runs the same app on the default and an `fs-confined` world and
  diffs `lstat` snapshots of source and copy.
- **Effectful callbacks cannot go through `List.fold_try`** (it takes a pure
  function): the copy and walk loops are explicit recursions.
- **Roc miscompile, not reduced:** a `Glob` helper
  `|path, prefix| { bytes = FilesPath.bytes(path); if cond { FilesPath.from_bytes(List.drop_first(bytes, 2)) } else { path } }`
  (where `FilesPath.bytes` is `Str.to_utf8` of the path's string) produced
  garbage paths across the whole result list. Rebuilding from `bytes` in both
  branches fixed it. Candidate: a refcount on the parameter returned from one
  branch while `Str.to_utf8`'s shared buffer is dropped in the other.
- **Roc builtins by their current names:** `List.fold`/`fold_try` (not
  `walk`), `List.set` answers `Try`, `U64.highest`, record patterns need `..`
  for unmentioned fields, typed bit ops (`U8.bitwise_and`, `U32.shl_wrap`), and
  an untyped `0` state defaults to `Dec`.

### Docs (step 14)

- READMEs for all three; trantor-process and trantor-files check theirs through
  `tests/readme` (trantor-hash's approach). trantor-cli's README examples are
  prose-only, as before.
- trantor-net and trantor-terminal pass `trantor test .` against the final
  trantor-cli.

### Independent review fixes (2026-09-15)

Three Opus reviewers, one per package; each finding was checked against the
code or reproduced before fixing.

- **trantor-cli `45557e8`:**
  - `write_via_stream!` seeks and writes at the shared cursor instead of
    `pwrite` at a private offset. Handing a writer's descriptor to a child
    made the child overwrite the stream's bytes, which the `pwrite` choice in
    step 2 caused.
  - A confined copy refuses a non-file and removes a partial destination.
  - Confined `rename_at`/`link_at` re-check a symlink they move (D-S2-22 below).
  - `open_flags` also rejects `create`/`exclusive` without `write` and neither
    `read` nor `write`; the OS answered EINVAL as `Other`.
  - `FdHandoff` duplicates start at fd 3.
  - `confined-race`: the swapper runs until killed, and the op count and the
    planted line are asserted.
  - Not changed: a directory descriptor is a stored path, so
    `follow_symlinks: False` holds only at open (documented on `open_at!`); an
    fd-based `Desc::Dir` would change how `resolve` confines.
- **trantor-process `5828290`:**
  - `stdin!`/`stdout!`/`stderr!` share the pipe inside the handle's borrow
    (use-after-free as a child's last use).
  - Stdin pipes get `F_SETNOSIGPIPE` on macOS (not in the libc crate: 73);
    `PipeWriter` also blocks SIGPIPE on the writing thread and consumes it,
    for Linux. Blocking alone did not stop the SIGPIPE on macOS.
  - `signal!` refuses a reaped child (`try_wait` first) and reads errno inside
    the borrow.
  - `collect!` input after `close_stdin!` is `NotPiped`.
  - New `tests/cmd-results` suite; the spawn suite has the regressions.
- **trantor-files `3a30df2`:**
  - Segment and token matching use last-star restart (linear); the
    adversarial corpus cases run in milliseconds.
  - A hidden component matches only a component pattern whose first token is a
    literal starting with `.`.
  - Class members are read escape-first, then ranged.
  - A trailing empty component is dropped.
  - A brace expansion that forms `**` where the text had none is `InvalidGlob`.
  - `literal_prefix` is `/` for absolute alternatives sharing only the root.
  - `expand!` searches through a symlinked start directory.
  - `Walk`:
    - `max_depth: 0` visits nothing.
    - A followed link aimed lexically at its own directory or an ancestor is
      not descended.
    - A followed link's listing is read once.
  - `Tree.copy!` makes links in rounds until a round makes none.
  - `Temp` deletes a file whose stream failed after the exclusive create.
- **D-S2-22 amended in effect:** the "renaming a relative link to another
  depth" hole is closed for the link itself; renaming a directory that holds
  relative links is still not checked.

### Second review round (2026-09-15; decisions D-S2-23..26)

- **trantor-cli `251aa20`:**
  - D-S2-23: `write_via_stream!` ignores `ESPIPE` from its seek; the shared
    cursor is documented on `Fs`.
  - D-S2-24: `link_stays_inside` returns early for absolute contents and for
    `from.parent() == to.parent()`, and maps a failed target check to
    `PermissionDenied`.
  - D-S2-25: `Fs.metadata_hash_at!(d, path, { follow_symlinks })` returns
    `{ lower, upper }`, two `DefaultHasher` passes over `(dev, ino)` salted 0
    and 1.
  - Tests:
    - `fs-write-modes`: a FIFO with a background `cat`, and identity through
      a link.
    - `confined-race` planted: allowed moves (absolute in place, dangling in
      place, relative across directories) and a refused dangling
      cross-directory move.
    - `fd-handoff`: runs again with `<&-`.
- **trantor-process `c8a66e2`:** D-S2-26 `without_sigpipe(fd, write)` is
  per-platform: on macOS `F_SETNOSIGPIPE` 1 before the write and 0 after, no
  `sigwait`; elsewhere the thread block and consume. The flag is no longer set
  at spawn. `tests/spawn` asserts `yes` into another child's stdin pipe ends
  `Signaled(13)`.
- **trantor-files `88c004a`:**
  - `Walk` threads `Inside : List(Identity)` when following. A directory, real
    or behind a link, whose identity is in it is not descended.
    `descent!` keeps a followed link's listing.
  - `GlobPattern.starts` groups alternatives by literal prefix;
    `Glob.expand!` walks each group and merges, sorting by bytes and dropping
    duplicates.
  - `GlobParse`: empty non-leading components dropped; `joins_stars` checks
    each brace join (unescaped trailing `*` meeting a leading `*`).
  - `prefix_from` gives `could_contain` the last-`**` restart.
  - `Tree.replace!` makes the new file or link at
    `<to>.trantor-replace-<random>` and renames it over; `make_room!` is gone.
- **Not testable here, left as notes:**
  - `copy_beneath`'s partial-copy removal: no read failure can be forced on a
    regular file.
  - The Linux SIGPIPE path: the development machine is macOS.

### Third review round (2026-09-15; decisions D-S2-27..28)

- **trantor-cli `724a267`:**
  - D-S2-27: `OpenFlags` gains `append` (glue record renamed:
    `AnonStructF1bdbf0aafca74e5`); `append` implies `write` in `open_flags`,
    like `exclusive` implies `create`; `append` with `truncate` or `directory`
    is `Unsupported`.
  - Both backends call `.append(f.append)`. `append_via_stream` reads
    `F_GETFL`: with `O_APPEND` the stream is the plain clone, otherwise an
    `EndWriter` seeks to the end per write. `FsOps.writer_at!` Append opens
    with `append: True`.
  - `tests/fd-handoff` hands off stdout before opening any file. Negative
    control run: with `FIRST_NON_STDIO_FD = 0` the stdin-closed run fails
    (`stdio-number:0`).
  - `fs-write-modes` asserts `XYllo!,XYllo!?`: an append stream then an
    offset stream on a plain descriptor, then an offset stream on an append
    descriptor.
- **trantor-process `ad19ba2`:** the pipeline check drops the parent's write
  into `head`'s stdin, and runs 5 pipelines requiring at least one
  `Signaled(13)`.
- **trantor-files `50f9a9f`:**
  - `descent!` treats `NotFound`, `NotADirectory` and `Other(_)` from the
    followed identity as leading nowhere (`IOErr` has no ELOOP variant), and
    defers a followed listing error as `Descend … Unlisted`.
  - `Glob.expand!` strips a leading `./` from every result before sorting and
    dropping duplicates.
  - `Tree.replace!` retries up to 8 random `.copy-in-progress-<16 hex>` names
    in `FilesPath.parent(to)` and deletes the temporary on any failure except
    `AlreadyExists`.
  - `FilesPath.hex` and `FilesPath.parent` are shared by `Temp` and `Tree`.
  - `walk-glob` gains loop, through-a-file, unlistable-at-max-depth,
    unlistable-skipped and `./` duplicate cases; `copy` gains a 240-byte name
    overwritten on both worlds.

### Fourth review round (2026-09-15; decisions D-S2-29..31)

- **trantor `f42086a` (`src/codegen.rs`):**
  - D-S2-29: the generated driver's `fill_closed_stdio` probes fds 0–2 with a
    `ManuallyDrop` `File::from_raw_fd(...).metadata()` for `EBADF` (9) and
    opens `/dev/null` read-write, leaking it onto the lowest free number.
  - `~/.bin/trantor` links to `target/release/trantor`, so the package suites
    use it after `cargo build --release`; trantor's `cargo test --release`
    passes (96).
  - The generated `main-driver` has 5 pre-existing `missing_safety_doc` clippy
    errors; the count is unchanged by this commit.
- **trantor-cli `fb7313b`:**
  - `EndWriter` skips `ESPIPE`.
  - `stream_result` takes `Option<RawFd>`; a seek-to-end append stream is
    minted with `output_stream` (no fd), so FdHandoff answers `NotAFile`
    (D-S2-31).
  - `fs-write-modes` appends through `fifo2` opened without `append`.
  - The `fd-handoff` stdin-closed run can no longer fail from the floor alone,
    since fd 0 is `/dev/null` now; its comment says so, and trantor-process's
    `<&-` spawn run covers the driver fill.
- **trantor-process `9b98c41`:**
  - `captured!` waits on the child when `collect!` fails with `Io`.
  - `sigpipe_run!` writes one byte into `head -c 2` before spawning `yes`, and
    all 3 runs must be `Signaled(13)`.
  - The spawn suite runs twice, the second time with `<&-`.
  - The regressions case asserts `ToStream` of a seek-to-end append stream is
    refused.
- **trantor-files `5aa49e5`:**
  - `Below.id` is `[Known(Identity), Unread]`; a real directory's identity is
    read at descent. `descent!` adds `PermissionDenied` to leads-nowhere
    (D-S2-30).
  - `Tree.relative` uses no separator after an empty root.
  - `replace_attempt!` answers `AlreadyExists` on the last collision without
    deleting.
  - `GlobPattern.literal_paths` and `Glob.existing!` check literal
    alternatives by name.
  - `without_dot_components` replaces `without_dot_slash`.
  - `expand_from!` treats `NotADirectory`, and `Other(_)` behind a link start,
    as no match.
  - Tests: `walk-glob` gains dot-components, unsearchable-dir, file-in-start,
    looped-start and literal-starts; `copy` gains copy-from-empty and a
    followed escape (`dir` unconfined, `link` confined).

### Fifth review round (2026-09-15)

- **trantor `835d8bc`:**
  - The driver's `fill_closed_stdio` declares `fcntl`/`open` itself and calls
    `fcntl(fd, F_GETFD)` (`EBADF` 9, `F_GETFD` 1) and
    `open("/dev/null", O_RDWR)` (2): no `O_CLOEXEC`, so children inherit the
    refilled fd.
  - The two-component golden (`tests/golden/two-component/golden/components/cli/src/lib.rs`)
    is regenerated; round four had left it stale.
- **trantor-process `58822e5`:**
  - `captured!` sends `Kill` before `wait!` after a failed `collect!`.
  - `tests/spawn`'s stdin-closed run is `capped 60 sh -c 'exec "$0" <&-' app`.
    `capped`'s perl opens a file on a free fd 0 before exec, so round four's
    `capped … <&-` never closed the app's stdin.
  - The first run gets `</dev/null`, and the regressions case runs
    `exec_output_inherit_stdin!` on `cat; echo rc=$?`, expecting `rc=0`.
    Negative control: with f42086a's driver the closed run reports `rc=1`.
- **trantor-files `1237ab1`:**
  - `GlobParse.segments` gives `[]` for an empty alternative.
  - `GlobPattern.starts` skips empty and unreachable alternatives.
  - `unreachable_literals` (last component `.`/`..`, or `/`) are the only
    ones name-checked, and `existing!` treats any check error as no match.
  - An all-literal start whose walk fails falls back to `existing!` on its
    `literal_paths`.
  - A link start is judged by `Path.list!(root)` alone before `search!`, whose
    errors propagate.
  - `walk-glob` fixtures:
    - `lit/xonly` (mode 111) lives under `lit/`, because at the top level it
      broke the `*/a.txt` pruning case.
    - `errdir/shut` (mode 000) sits behind link `lerr`.
- Case-insensitive filesystems: literals the walk reaches go through the
  case-sensitive matcher; the fallback name check is filesystem-resolved, as a
  shell's literal word is.

### Sixth review round (2026-09-15; decisions D-S2-32..35)

- **trantor-files `22e14e5`:**
  - `Segment` gains `DirectoryOnly`, appended by `GlobParse.segments` for a
    trailing empty component. `GlobPattern.content` strips it before matching
    or literal checks. `match_kind` answers `[NoMatch, Match, IfDirectory]`,
    and `search!`/`existing!` resolve `IfDirectory` with `is_directory!`
    (`IsDir`, or a link that lists) (D-S2-33).
  - `no_dots_after_wildcards` makes `.`/`..` after a non-literal segment
    `InvalidGlob` (D-S2-32).
  - `existing!` returns `Try`: `NotFound`, `NotADirectory` and
    `FilesPath.is_link_loop` (message containing `(os error 62)` or
    `(os error 40)`) are no match; other errors fail (D-S2-34).
  - The start `lstat` and link-start listing treat a loop, and for a link
    `PermissionDenied`, as no match (D-S2-35).
  - `starts` keeps literal alternatives in their own group. `{xo/f,xo/*}`
    still fails, correctly: `xo/*` cannot be expanded.
  - Tests: `walk-glob` covers directories-only, a file with a slash, `..`
    after a wildcard, and a loop in a start prefix. `copy` checks
    `../outside/f`, `../outside/*` and `outlink/*` on both worlds
    (unconfined: found; confined: `PermissionDenied`, `PermissionDenied`,
    none).
- **trantor-process `55672d8`:**
  - `check_available!` uses `default_search_path` `/usr/bin:/bin` for an
    unset PATH.
  - `explain_missing_cwd` turns a spawn's `NotFound` into
    `Other("the working directory … no longer exists")` when the userland cwd
    is not a directory.
  - `collect` borrows the input slice (decref after), drops each output `Vec`
    after its `RocList` copy, and `stop_on_failure` SIGKILLs the unreaped
    child when a reader fails.
  - `cmd-results` runs again under `env -u PATH` and checks the removed cwd.
  - Untested here: the reader-failure kill (no pipe read error can be forced).
- **All three `tests/lib.sh` (`55672d8`, `1d2ec5b`, `82e6979`):** `capped` is
  `exec { $ARGV[0] } @ARGV or die`. trantor-cli's Fs `interface.toml`
  comment now describes kinded listings.

### Seventh review round (2026-09-15; decisions D-S2-36..37)

The trantor-cli reviewer ran the APIs end to end on both backends this round,
and found older backend disagreements that reading diffs had not.

- **trantor-cli `32bfd6f`:**
  - D-S2-36: `fs-core/src/copy.rs` holds the one copy. `open_source` and
    `create_destination` (`create_new`, mode without `0o7000`) are the only
    per-backend code; `io::copy`, `set_permissions`, `remove` on failure. The
    source is checked before the destination.
  - `relative_to_root` re-appends a trailing `/` that `strip_prefix` dropped.
    `refuse_directory_name` makes `write_file_at`, the copy destination,
    `symlink_at`'s link, `link_at`'s destination, and a rename of a
    non-directory to or from a slash-terminated name `NotADirectory` on both
    backends. A cap-std probe showed `write("out/")` gives `InvalidInput`
    and `rename(f, "k3/")` succeeds, where the kernel says `NotFound`.
  - `dir_of` returns `Result`; a `Desc::File` base is `NotADirectory`.
  - `read_via_stream` mints a `Failed` reader for a failed clone.
  - `StrPath.read_sym_link!` errors on a non-UTF-8 target. README fixed.
  - New `tests/backends-agree`: 10 operations, same app on both worlds,
    outputs diffed; then modes (setuid dropped), mtimes (not 2000),
    `write-slash err:NotADirectory`, `copy-missing err:NotFound`,
    `file-base err:NotADirectory`.
  - A link_at guard first written as `_ if … &b` moved `b`; the build error
    surfaced as `trantor new` failing in `argv-bytes`.
- **trantor-files `e8780bf`:**
  - D-S2-37: the link-start listing no longer treats `PermissionDenied` as no
    match.
  - `is_directory!` opens a link with `Fs.open_at! { directory: True }`, and
    counts `PermissionDenied` as a directory.
  - `split_components` makes `\/` `InvalidGlob`.
  - `walk-glob` gains `perm/` fixtures (`zdir` 000, `xdir` 111, links to
    each and to a file).
  - `copy`'s confined `outlink/*` now expects `err:PermissionDenied`.
- **trantor-process `38b75fa`:**
  - `path_dirs` maps an empty Unix entry to `.` and drops Windows empties.
  - `cmd-results` runs from `$TMP`, and has a `check` mode run under
    `env PATH=/nope:` from a directory holding `local-tool`, expecting
    `True local-tool`.

### Eighth review round (2026-09-15; decisions D-S2-38..41)

Three reviewers ran probe apps on both backends. None of the findings changed
a recorded decision, but three of the fixes needed one: D-S2-38, D-S2-39 and
D-S2-41. D-S2-40 came up during implementation: executability can only follow
links if `stat_at!` can.

- **trantor-cli `ff906b2`, `eca456e`, `6951d74`:**
  - `relative_to_root` re-appends the trailing spelling that `strip_prefix`
    drops, through `trailing_spelling`: `/.` if a `.` component ended the path,
    otherwise `/`. `names_directory` (ends in `/` or `/.`) replaces
    `ends_in_slash`. Before the fix, confined `keep.txt/.` overwrote the file
    and `filelink/.` deleted the link.
  - `beneath_lstat` follows a link for a directory name. It is used by
    `stat_of`, by `metadata_hash_at` without follow, and by the rename check;
    cap-std's `symlink_metadata("dirlink/")` is ENOTDIR.
  - `open_non_directory` refuses create or exclusive at a directory name
    (`NotADirectory`, not `NotFound`/`EINVAL`).
  - `copy.rs`: `open_source` uses `O_NONBLOCK` (a FIFO hung both backends in
    open). `remove_if_same` removes a partial copy only if (dev, ino) still
    matches the created file.
  - D-S2-41: `sync_io_core::Handoff`, `input_stream_failed`. The `Failed`
    reader moved from fs-core to sync-io-core. `dup_owned` returns the
    result, `handoff_io` builds `IoOrNotAFile::Io(FdHandoffIOErr)`: the glue
    names, confirmed by building.
  - D-S2-39: `copy_dir_at!` in `copy.rs` (`copy_dir`, `create_dir` with
    `DirBuilder` mode 0o700, `O_DIRECTORY|O_NOFOLLOW` or `open_dir_nofollow`,
    `set_permissions`, and `remove_dir` if that fails). Exposed as
    `FsOps.copy_dir!`.
  - D-S2-40: `stat_at!` takes `HashFlags`; `FsOps.executable!` follows.
  - `backends-agree` grows to 33 operations:
    - `/.` cases, `dirlink` `/` and `/.` kinds, the hash;
    - `sub/q/` rename (the link target moves, and the link stays);
    - create at `oc/`, a FIFO copy;
    - `copy_dir_at!` on 0700/0555/2750 directories, a link and a file source,
      an existing destination;
    - executability of dangling, non-executable and executable links.

    The script checks modes 700/755/750 under `umask 022`. `fd-handoff` adds
    an `exhaust` mode (opens until refused under `ulimit -n 64`) expecting
    `io`.
- **trantor-process `517915c`:**
  - Linux `without_sigpipe` records whether SIGPIPE was pending before the
    write. It takes a newly pending one with a zero-timeout `sigtimedwait`,
    whatever the write returned; `sigpipe_pending` is the helper. Checked
    with `cargo check`/`clippy --target x86_64-unknown-linux-gnu`; it cannot
    run here.
  - `collect` kills on a reader failure only if `try_wait` showed the child
    unreaped at entry.
  - `Subprocess.first_failure` picks spawn's error from the redirects.
  - `Cmd.not_directory!` checks `candidate.join(".")`.
  - Docs: `wait!` on unread pipes, `collect!` with a grandchild on stdin.
  - `cmd-results` `check` mode covers five tools: plain, dangling link, link
    to 0644, link to the tool, link to a directory. Each must agree between
    `check_available!` and a spawn.
- **trantor-files `3ac4aa2`, `b9d6277`:**
  - D-S2-38: `is_directory!` falls back to `metadata_hash_at! {follow}` on
    `PermissionDenied`.
  - `Temp.after_cleanup` treats `NotFound` as a clean cleanup.
  - `Tree.make_dir!` takes the source and calls `FsOps.copy_dir!`.
  - `Walk.descent!` stays only on `is_link_loop` among `Other` errors.
  - `GlobParse.class` returns `Try`: a reversed range and an un-negated
    class of only `/` are `InvalidGlob`.
  - Tests:
    - `walk-glob` gains `p2/` (`shut` 000, `lfile`, `ldang`) and two class
      cases;
    - `temp` gains `moved!`;
    - `copy`'s snapshot prints directory modes, and its fixture has
      `src/private` 700.
  - Docs: `Tree`, `Glob.expand!`, README.
- A trantor-terminal run alongside trantor-net failed the two pty signal tests
  again. Run alone, it passed. trantor-net and b8 pass.

### Ninth review round (2026-09-15; decisions D-S2-42..43)

Four findings were regressions from rounds 7 and 8: directory names ending in
`/` confined, `copy_dir_at!`'s confined source, a walk over a link to an
over-long name, and a temp leak. Two were older: redirect fds an app could
close, and a stdin deadlock.

- **trantor-cli `a5c4b3c`, `2fad58c`:**
  - fs-core directory names:
    - `slashes_stripped` (a name ending in slashes and not `.`/`..`).
    - `Target::with_path`/`path` (replace `target_path`).
    - `existing_directory_name` for `remove_dir_at`/`remove_dir_all_at`: a
      link or file is `NotADirectory`. Confined `remove_dir("tree/")` had
      been EINVAL after emptying the tree, and macOS removed a link's target.
    - `new_directory_name` for `create_dir_at` and `copy_dir`: anything at
      the name is `AlreadyExists`, where macOS made `dangling/`'s target.
  - `names_directory` no longer counts a bare `.`: the root named without a
    slash gets the OS's answer on both backends. Confined exclusive creation
    over a directory maps cap-std's `IsADirectory` to `AlreadyExists`
    (`root-copy-onto`).
  - `open_non_directory`: `IsADirectory` for an existing directory
    (`metadata_followed`).
  - `copy_dir`:
    - reads the source through `metadata_as_named` (`beneath_lstat`);
    - makes and opens the stripped name (`make_owner_only`, `open_made`), so
      `O_NOFOLLOW` applies;
    - removes the directory if the open or chmod fails, so a `umask 0400`
      probe leaves nothing.
  - `cli-host` `SharedStdin`: locks `stdin()` per `read`, holds the lock only
    from `fill_buf` to `consume`, and drops it on a failed fill.
  - Tests:
    - `backends-agree` +13 cases (46): slash removals, links, dangling mkdir,
      copy_dir slash source and destinations, create at `adir/`, root
      write/copy/symlink;
    - `confined-race` +`copy_dir` and `stat_followed` (12 ops);
    - new `tests/stdin-streams`: two streams and `Stdin.line!` interleaved,
      `1 2 3 4 5`.
- **trantor-files `10892cc`:**
  - D-S2-42: `FilesPath.is_name_too_long` (63/36) and `leads_nowhere`, used
    by `Walk.descent!` and Glob's three sites.
  - `Temp.with_*_in!` resolve the made path at creation, and clean up by it
    through `FsOps.delete!`/`delete_all!`.
  - `GlobParse.class` also refuses `Range('/', '/')`.
  - Docs: `Tree.copy!` and the README say a failed copy leaves what it made.
  - Tests: `walk-glob` adds `long/toolong` (walk, glob through, 300-byte start,
    literal in braces) and `[/-/]`; `temp` adds `moved_cwd!`.
- **trantor-process `47211ed`:**
  - `redirect!`, `release!` and `first_failure` move to module level.
    `spawn!` releases its FdHandoff fds after `spawn_redirected!` either way.
  - The host's `stdio` duplicates an `Fd` (`F_DUPFD_CLOEXEC` ≥ 3) and never
    closes the caller's.
  - D-S2-43: the `can_execute!` leaf (OsStr; `executable_by_user` in lib.rs).
  - `Cmd`:
    - `candidate_available!` returns `[Found, Missing, Stop]`, and
      `search_*` take `os`;
    - `not_directory!` counts only `PathErr(NotADirectory)` as a file. `Path`
      here is basic-cli-lib's, whose errors are `PathErr`.
  - `collect` makes threads with `Builder::spawn_scoped`. On a failed reader
    spawn it kills an unreaped child and joins the writer (`kill_unreaped`).
    Docs: `collect!` after `Io`, `check_available!`.
  - Tests: `spawn` adds a `borrowed` mode (`Fd(1)` to the raw leaf, then
    `Stdout.line!`: `child parent`); `cmd-results` adds `group-only` (mode 654).
- trantor-net, trantor-terminal (run on its own) and b8 pass.

### Tenth review round (2026-09-15; decisions D-S2-44..45)

Two round-9 regressions (the stdin stream's end of input, and `create_all!`),
one confined-world break in `check_available!`, and a quadratic glob.

- **trantor-cli `629e376`:**
  - `cli-host` `SharedStdin::fill_buf` decides on the first fill: an empty or
    failed one drops the lock and returns, where a second `fill_buf` issued
    another `read(2)` and waited for a second Ctrl-D on a terminal.
  - `create_dir_all_at` runs `new_directory_name`, as `create_dir_at` does.
  - `renamed_source` replaces the rename guard's `unwrap_or(false)`: a failed
    stat answers with its own error.
  - `read_via_stream` on a directory mints `input_stream_erroring`
    (`IsADirectory` reads, `Handoff::NotAFile`), new in sync-io-core beside
    `input_stream_failed`.
  - D-S2-44: `OpenFlags` gains `mode` (the glue hash moved to
    `AnonStructA38ac8acc52dafef`), `create_dir_at!` takes a mode,
    `Fs.default_file_mode`/`default_dir_mode` hold the defaults, and
    `FsOps.create_dir_mode!` is the byte-level door. `copy.rs` exposes
    `make_dir`.
  - Tests: `backends-agree` +11 (57): `create_all!` over a dangling link, a
    link and a file; rename over an unreadable and a missing name; a named
    file and directory mode; a directory descriptor's stream.
    `stdin-streams` gains `on_a_terminal.py`, a pty that sends one Ctrl-D —
    it fails against the old `fill_buf`, which is how the regression was
    confirmed. `.gitignore` now covers `__pycache__`.
- **trantor-files `6a626d4`:**
  - D-S2-44: `Temp` opens files with `mode: 0o600` and makes directories
    through `FsOps.create_dir_mode!(…, 0o700)`.
  - `GlobPattern.starts` folds with a `Dict` of group keys (`l`/`g` plus the
    prefix) and carries `literal` on the group, instead of re-deriving it per
    alternative: `{a,b}` ×15 went from 87s to 3s, ×16 from ~350s to 12s.
    (`Dict.empty()` takes no argument in this compiler.)
  - `Walk`'s doc lists the name-too-long case; `GlobText` bounds its 3-byte
    branch at `0xF0`.
  - Tests: `walk-glob` runs `{a,b}` ×15 under `capped 30`; `temp` checks 600
    and 700 under `umask 022`.
- **trantor-process `adacda9`:**
  - `Cmd.not_directory!` asks `Subprocess.can_execute!(candidate/.)`, not
    `Path.is_dir!`: under `fs-confined` every candidate outside the root was
    `PermissionDenied`, so every program looked missing.
  - `executable_by_user` also requires a regular file (`execve` gives EACCES
    otherwise), so an executable FIFO on PATH is not available.
  - `collect` kills an unreaped child when the writer thread cannot start.
  - D-S2-45: `interfaces/subprocess-raw/` (module `SubprocessRaw`) holds
    `spawn_redirected!`, `Redirect`, `Handle` and `Cmd`; `package.toml` wires
    it to subprocess-host and leaves it out of `exports`. Glue renamed the
    twins: the raw spawn's is `SubprocessRawIOErr`, `handle_wait!`'s is
    `SubprocessIOErr` (hence `wait_ioerr`), and the rest stay `IOErr`.
  - Tests: `spawn`'s `borrowed` mode lends stdout through `ToStream` and
    checks an app naming `SubprocessRaw` does not build; new
    `tests/confined-available` (`True ran` with `fs-confined` wired).
- trantor-net, trantor-terminal and b8 pass.

### Eleventh review round (2026-09-15; decision D-S2-46)

Three round-10 regressions (two of them fixes that did not go far enough), and
two performance cliffs a user would hit.

- **trantor-cli `91f52d4`:**
  - `create_dir_all_at` is idempotent again: a directory already at the name,
    followed as the kernel follows `dir/`, is success. Round 10 gave it
    `new_directory_name`, so `create_all!("out/")` worked once and then
    answered `AlreadyExists`, and `create_all!("dirlink/")` never worked.
    The refusals round 10 added (`dangling/`, `filelink/`, `keep.txt/`) still
    hold, since none of them follows to a directory.
  - `backends-agree` +3 (57 operations): existing directory, link to one,
    and a repeat.
  - README lists `mode` among the open flags.
- **trantor-files `85c7078`:**
  - `Walk.descent!` decides a followed link's kind with `Fs.stat_at!`
    (following) instead of listing the target: `max_depth: 1` over 200 links
    to 20,000-entry directories went from 3.0s to about 0.2s, and `SkipDir`
    can now avoid the work. `Below` loses its `listing`, and `Walk.Entry`
    gains `link : Bool`.
  - D-S2-46: `search!` walks with `follow_symlinks: True`;
    `could_contain_through_link` (a `globstar` flag threaded through
    `prefix_from`) keeps `**` from reaching through a link. The walk-glob
    expectation for `*/a.txt` now includes `linked/a.txt`, and three cases
    cover a link under a wildcard.
  - `starts` groups in two append-only passes (number each alternative, sort
    by group index, then `collected` builds each group's list while it is
    uniquely owned): 131072 alternatives went from 144s to about a second.
    Round 10's note claiming the quadratic was gone was wrong; the Dict
    removed the scan, not the nested-list copy. The test now runs `{a,b}`
    ×17 under `capped 30`. (`List.sort_with` takes a `[Before, Same, After]`
    comparator; there is no `U64.compare`.)
  - `GlobText`'s two-byte branch is bounded at `0xF8`, which round 10's fix
    missed. `Temp` names paths with `FilesPath.child`, so a `TMPDIR` ending
    in `/` (the macOS default) no longer doubles the separator; the temp test
    sets one.
- **trantor-process `d4dfb89`:**
  - `candidate_available!` drops `not_directory!` on Unix (D-S2-40 amended);
    `windows_not_directory!` keeps the `Path.is_dir!` check for the Windows
    arm, which cannot reach the Unix-only leaf.
  - `steps_past` adds ESTALE (116), ENODEV (19) and ETIMEDOUT (110) to what
    the Linux search walks past.
  - Docs: `Subprocess.Cmd`'s flat `envs` list on the exported surface, and
    `collect!`'s SIGKILL, which does not follow every `Io`.
  - `cmd-results` checks a tool at a 1022-byte path.
- trantor-net, trantor-terminal and b8 pass.

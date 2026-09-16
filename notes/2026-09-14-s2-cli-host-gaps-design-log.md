# S2 — trantor-cli host gaps: design log (2026-09-14)

**Plan:** [`plans/2026-09-14-s2-cli-host-gaps.md`](../plans/2026-09-14-s2-cli-host-gaps.md).
**Roadmap:** D-S1-11 step 2 in
[`2026-09-14-s1-stdlib-roadmap-design-log.md`](2026-09-14-s1-stdlib-roadmap-design-log.md).

Settled against trantor-cli `1c51dc7`. The brief: temp files, copy, recursive
walk and glob, append and streaming writes, and subprocess spawn with pipes,
added to the package that follows the basic-cli port's principles
([`2026-09-04-basic-cli-port-design-log.md`](2026-09-04-basic-cli-port-design-log.md),
P1–P15).

## What was found before any question was asked

- **Two surfaces.** The raw layer (`Fs`, `Streams`, `Subprocess`) is
  WASI-shaped and descriptor-relative (P1, P4). The basic-cli shim (`Path`,
  `File`, `Cmd`) is pure Roc over it through `FsOps` and `Host` (P15).
- **`Fs.open_at!` takes a `U8`:** 0 = read, 1 = create + truncate. No
  append, no exclusive create, no write stream; `write_file_at!` writes whole
  files. `read_via_stream!` is the only stream.
- **`Subprocess` has only run-to-completion calls:** `exec_exit_code!`,
  `exec_status!`, `exec_output!`, `exec_output_inherit_stdin!`, each one host
  call sharing `command()` (userland cwd, env, the signal-mask reset from
  `1c51dc7`). `exec_output!` is `Command::output()`, whose threads make the
  two-pipe deadlock impossible today.
- **Their only caller is `Host`'s `cmd_exec_*` pass-throughs, whose only
  caller is `Cmd`.** Nothing outside trantor-cli calls `Subprocess.exec_*`.
- **rocjust fakes a pipe.** `--choose` runs `sh -c "… < '/tmp/in' > '/tmp/out'"`
  with pid-named temp files because basic-cli's `Cmd` cannot pipe to a
  child's stdin (`rocjust/app/main.roc:4181`).
- **Confinement is `fs-core`'s `Root`:** every path goes through `resolve` and,
  when confined, cap-std. A new host filesystem op has to do the same or
  `fs-confined` stops confining. A spawned child is not confined at all; a
  confined world stays confined only by not wiring `subprocess`.
- **Optional record fields exist (`x ?: T`) but may not cross the host
  boundary** ("Host-bound types cannot contain `?:` record fields"). Interface
  modules may carry Roc bodies (`OsStr.roc`).
- **Component isolation:** `Fs.Descriptor`'s payload is `fs-core`'s `Desc`;
  `OutputStream`'s is `sync-io-core`'s `Output(Box<dyn Write>)`.
  `subprocess-host` can read neither. The only cross-component reach today is
  the named extern `trantor__cwd_host__get`.

## Decisions

### D-S2-1 Host primitives only where WASI or the OS is needed; the rest is derived Roc

New host ops: WASI-shaped `open_at!` modes, `write_via_stream!`,
`append_via_stream!`, entry kinds from `read_dir_at!`, `copy_file_at!`
(D-S2-10), `readlink_at!`, `symlink_at!`, and subprocess spawn. Temp files,
walk, glob and copy-tree are Roc over those. (User.)

**Why:** P1/P4 — primitives WASI-shaped, basic-cli's API derived. Every new
host op passes through `Root::resolve`; derived features inherit confinement.

**Rejected:** every gap a host op (each a new confinement surface); shim-only
leaves (the raw layer permanently weaker than the shim).

### D-S2-2 `open_at!` takes WASI's flags as one record with defaults

A Roc `Fs.open_at!` over a hosted leaf that takes the complete record. (User:
"a single struct with defaults".)

```roc
open_at! : Descriptor, List(U8), {
    read ?: Bool, write ?: Bool, create ?: Bool, exclusive ?: Bool,
    truncate ?: Bool, directory ?: Bool, follow_symlinks ?: Bool,
} => Try(Descriptor, [Io(IOErr)])
write_via_stream! : Descriptor, U64 => Try(Streams.OutputStream, [Io(IOErr)])
append_via_stream! : Descriptor => Try(Streams.OutputStream, [Io(IOErr)])
```

WASI's sync flags (`file-integrity-sync`, `data-integrity-sync`,
`requested-write-sync`, `mutate-directory`) are left out until something needs
them (P6).

**Rejected:** WASI's three separate flag records; a bit field; purpose-built
`create_new_at!`/`open_append_at!` leaves.

### D-S2-3 Open defaults and validation

`read` and `follow_symlinks` default `True`, the rest `False`, so `{}` opens
an existing file read-only. `exclusive` implies `create`. `truncate` without
`write`, and `directory` with `write`/`create`/`truncate`, return
`Err(Io(Unsupported))` in Roc before the host. (User.)

**Why:** read-by-default matches every mainstream `open`; rejecting in Roc
gives one error on every OS. The hosted leaf still takes WASI's full set, so
real WASI stays a boundary mapping.

**Rejected:** WASI's all-`False` defaults; no validation.

### D-S2-4 Write streams are unbuffered

Every `Streams.write!` is one `write(2)`; there is no `flush!`. (User.)

**Why:** a resource destructor cannot return an error to Roc, so a buffer
flushed at drop loses the disk-full that happens there. Unbuffered, every error
comes back from the write that caused it. Programs batch in Roc when syscall
count matters.

**Rejected:** a host `BufWriter` with `flush!` (errors silently lost when
forgotten); a crash on unflushed drop.

### D-S2-5 Shim and path packages get append and a `Writer`

`Path.append_bytes!`/`append_utf8!`; `File.Writer` from `open_writer!`
(create + truncate) and `open_append!`, with `write!`, `write_utf8!`, `line!`.
`OsPath` and `StrPath` get `append_bytes!`, `open_writer!`, `open_append!`.
(User.)

**Why:** mirrors the existing `File.Reader`; additions keep basic-cli
programs compiling (P15).

**Rejected:** append only; raw layer only.

### D-S2-6 Temp files are scoped, with manual variants

`Path.with_temp_dir!`/`with_temp_file!` delete on both `Ok` and `Err`;
`create_temp_dir!`/`create_temp_file!` return a path the caller deletes.
Derived: a name from `RandomHost.seed_u64!`, exclusive create, retry on
`AlreadyExists`. A crash leaves the file. (User.)

**Rejected:** a host resource deleting on release (deletion at a refcount
moment, and a host primitive against D-S2-1); manual only.

### D-S2-7 Temp location: `Env.temp_dir!()` plus `_in` variants

Every temp function has an `_in(base)` form. Under `fs-confined` the operator
sets `TMPDIR` inside the root or the app passes a base. (User.)

**Why:** no preopen change; `TMPDIR` is the lever Unix tools already honor;
`_in` covers rocjust's `--tempdir`.

**Rejected:** named preopens with a `"tmp"` preopen (right for WASI, left for
later); `<root>/.tmp` under confinement.

### D-S2-8 Walk is an effectful fold, plus a list wrapper

```roc
Path.walk! : Path, state, { follow_symlinks ?: Bool, max_depth ?: U64 },
    (state, { path : Path, kind : [IsFile, IsDir, IsSymLink, IsOther], depth : U64 }
        => [Continue(state), SkipDir(state), Stop(state)])
    => Try(state, [PathErr(IOErr), ..])
Path.walk_list! : Path, { follow_symlinks ?: Bool, max_depth ?: U64 } => Try(List(Entry), …)
```

Depth-first, names sorted within a directory, symlinks not followed by default.
An unreadable subdirectory fails the walk. `read_dir_at!` changes in place to
return each entry with its kind (WASI's `directory-entry`), so a walk is one
call per directory; `FsOps.list!` and the path packages move with it. (User.)

**Rejected:** an eager list only (no pruning, whole tree before stopping).

### D-S2-9 Glob: globset syntax, walked from the literal prefix

`*`, `?`, `**` (a whole component, zero or more directories), `[…]`,
`[!…]`, `{a,b}`, `\` escapes; `*`/`?` do not match a leading `.` unless the
pattern does. `Glob.matches : Str, Str -> Bool` is pure, in its own component
(may move to `trantor-text` at S1 step 5). `Path.glob! : Str => Try(List(Path),
[PathErr(IOErr), InvalidGlob(Str), ..])` walks from the longest literal prefix,
resolves relative patterns against the userland cwd, prunes with `SkipDir`,
returns names sorted, and `Ok([])` for no match. (User.)

**Rejected:** POSIX `fnmatch` (no `**`); gitignore semantics (ignore-file
matching, not expansion).

### D-S2-10 Copy is a host op per file; trees are derived

`Fs.copy_file_at! : Descriptor, List(U8), Descriptor, List(U8) => Try({},
[Io(IOErr)])` — `std::fs::copy` unconfined, cap-std `Dir::copy` confined.
Copy-tree is walk + `copy_file_at!` + `create_dir_at!`. An existing destination
fails with `AlreadyExists` unless `{ overwrite: True }`. Symlinks are copied as
links through new `readlink_at!` and `symlink_at!` (WASI has both).
Timestamps are not preserved. (User.)

**Why:** a stream copy drops the executable bit, and the raw layer cannot set
permissions — a correctness gap, which D-S2-1's "derive unless measured"
did not cover. One host call gets the bits and the OS fast paths, and cap-std
already confines it. This amends D-S2-1 for copy only.

**Rejected:** a `set_permissions_at!` primitive with a derived copy; a derived
copy documented as dropping permissions.

### D-S2-11 `Stdio` per stream

```roc
Stdio : [Inherit, Null, Pipe, Descriptor(Fs.Descriptor), ToStream(Streams.OutputStream)]
spawn! : Cmd, { stdin ?: Stdio, stdout ?: Stdio, stderr ?: Stdio } => Try(Child, [Io(IOErr)])
```

All default `Inherit`. `Descriptor` redirects to or from an opened file without
a shell; the same descriptor on stdout and stderr merges them. (User; the
variant was named `FromFile` when chosen and renamed because it serves outputs
too.)

**Rejected:** `Inherit | Null | Pipe` only; an extra merged-output variant.

### D-S2-12 `Child`: raw pipes plus `collect!`

```roc
Child :: Box(U64)
Child.pid : Child -> I32
Child.stdin! / stdout! / stderr! : Child => Try(stream, [NotPiped])
Child.close_stdin! : Child => {}
Child.wait! : Child => Try(ExitStatus, [Io(IOErr)])
Child.try_wait! : Child => Try([Running, Done(ExitStatus)], [Io(IOErr)])
Child.signal! : Child, [Term, Kill, Int, Hup, Quit, Usr1, Usr2] => Try({}, [Io(IOErr)])
Child.collect! : Child, List(U8) => Try({ status : ExitStatus, stdout : List(U8), stderr : List(U8) }, [Io(IOErr)])
ExitStatus : [Exited(I32), Signaled(I32)]
```

Pipes are OS pipes (streaming, bounded memory). `collect!` writes stdin, reads
both outputs to end on host threads, then waits — so piping both outputs has a
safe path, and the deadlock rule is documented for hand-streaming. Dropping a
`Child` neither kills nor reaps it. `ExitStatus` is a union, so no negated-signal
encoding in new code. (User; `Exited` widened from `U8` to `I32` for Windows'
32-bit codes when checking D-S2-13.)

**Rejected:** host drains every output into an unbounded buffer; at most one
piped output.

### D-S2-13 The four `exec_*` host calls are removed; `Host`'s adapters go

`Subprocess` becomes `spawn!` and the `Child` operations. `Host`'s
`cmd_exec_*` pass-throughs are deleted and `Cmd` calls `Subprocess` directly.
`Cmd`'s public functions keep their signatures and errors. (User.)

The rebuild preserves every observable result:

| Call | Over the new primitives |
|---|---|
| `exec_exit_code!` | `spawn!(cmd, {})` + `wait!`; `Signaled(n)` → `Err(Other("child was killed by signal n"))` |
| `exec_status!` | same; `Signaled(n)` → `Ok(-n)` |
| `exec_output!` | `spawn!` with `stdin: Null`, both outputs `Pipe`, `collect!(child, [])`; `Signaled(_)` → `exit_code: -1` |
| `exec_output_inherit_stdin!` | same with `stdin: Inherit` |

Rules: `collect!` reads both outputs to end then waits (a grandchild holding a
pipe blocks it, as it blocks `output()`); `collect!` with stdin not piped
accepts only `[]` and otherwise returns `NotPiped`; the `-1` and `-N` mappings
stay as they are today.

**Why:** one path to process creation, so a fix like `1c51dc7` lands once, and
the raw interface is the actual primitive set.

*(Location superseded by D-S2-19: `Subprocess`, `Cmd` and this rebuild live in
`trantor-process`.)*

**Rejected:** keeping the four calls beside `spawn!` (two paths to keep in
agreement); keeping them in the interface but implemented on the spawn code.

### D-S2-14 File handles reach `subprocess-host` through an internal interface

A hosted `raw_fd!` for descriptors, and one for streams that have an fd
(files, stdio; `NotAFile` otherwise), in an interface module the package does
not export (e.g. `FdHandoff`). `Subprocess`'s Roc side passes the number to
`spawn!`, which `dup`s it into the child. The README states that a spawned
child is not confined and that a confined world stays confined by not wiring
`subprocess`. (User.)

**Why:** the dependency stays visible to trantor's wiring, with no fixed extern
both fs components must agree on. The descriptor already passed the confined
open.

**Rejected:** a fixed-name extern exported by the fs components; dropping
`Descriptor`/`ToStream`.

### D-S2-15 `Cmd.spawn!` returns the raw `Child`

`Child`, `Stdio` and `ExitStatus` live in `Subprocess`.
`Cmd.spawn! : Cmd, { stdin ?: Stdio, stdout ?: Stdio, stderr ?: Stdio } =>
Try(Subprocess.Child, [SpawnFailed({ command : Str, err : IOErr }), ..])`.
`File.Reader.stdio` and `File.Writer.stdio` give a `Stdio.Descriptor`. A
`Reader`'s already-buffered bytes are not seen by a child given its descriptor.
(User.)

**Rejected:** a shim `Cmd.Child` wrapper; raw `spawn!` only.

*(`Reader.stdio`/`Writer.stdio` superseded by D-S2-21.)*

### D-S2-16 Names follow basic-cli

`copy!`, `copy_all!` (`{ overwrite ?: Bool }`), `sym_link!` (target, link),
`read_sym_link!`, `walk!`, `walk_list!`, `glob!`, `with_temp_dir!`,
`with_temp_file!`, `create_temp_dir!`, `create_temp_file!` (+ `_in`),
`append_bytes!`, `append_utf8!` — on `Path`, and the same on `OsPath` and
`StrPath`. (User.)

**Rejected:** Rust `std::fs` or Python names.

*(Superseded for walk, glob, temp and copy-tree by D-S2-20.)*

### D-S2-17 Tests

Unit expects (glob corpus; open defaults and rejections; `ExitStatus`
mappings); five new integration suites (`fs-write-modes`, `temp`, `walk-glob`,
`copy`, `spawn`); the existing `three-defects`, `relative-cwd` and
`child-signal-mask` as regressions; the `confined-race` suite extended to every
new filesystem op, including a planted symlink. No performance or snapshot
tests. (User.)

### D-S2-18 The additions do not all belong in the baseline

(User: "These additions don't strike me as necessary to be in the core CLI
module.") Found when asked: an add-on can define interfaces and import
trantor-cli's (trantor-net's `sockets-host` imports `sync-io`), but cannot add
operations to an interface trantor-cli owns or read another component's
resource payloads; and Roc methods must live in the type's own module.

So the `Fs` primitive changes (D-S2-2, -8's entry kinds, -10's
`copy_file_at!`/`readlink_at!`/`symlink_at!`) and `FdHandoff` (D-S2-14) must
stay in trantor-cli; everything derived may leave.

### D-S2-19 Process creation leaves the baseline: `trantor-process`

`Subprocess` (`spawn!`, `Child`, `collect!`, `Stdio`, `ExitStatus`), `Cmd`
rebuilt on it (D-S2-13 holds inside the package), `subprocess-host`, and the
`three-defects`, `relative-cwd` and `child-signal-mask` suites move to a new
add-on package depending on trantor-cli. (User.)

**Why:** (user) a baseline that can spawn is a trivial confinement escape. The
escape predates `spawn!` — `Cmd.exec!("sh", ["-c", …])` already reaches
anything, because trantor-cli wires `subprocess` by default — so the fix is
removing process creation from the baseline, making a confined world confined
by not adding a dependency. Matches `Tty` leaving for trantor-terminal
(`98c97de`). Nothing else in trantor-cli uses subprocess; `subprocess-host`
still reaches the userland cwd through `cwd-host`'s extern.

**Cost:** a basic-cli app using `Cmd` adds one `[deps]` line, which departs
from P15's URL-only migration.

**Rejected:** keeping it in trantor-cli but unwired by default (`Cmd` exported
from a package that cannot run it); keeping it wired with a README warning;
keeping only `spawn!` in the baseline (the escape stays).

### D-S2-20 Methods for single operations, `trantor-files` for features

Methods on trantor-cli's `Path`, `OsPath` and `StrPath`: `append_bytes!`,
`append_utf8!`, `copy!` (one `copy_file_at!`), `sym_link!`,
`read_sym_link!`, and the existing `hard_link!`; `File.Writer` with
`open_writer!`/`open_append!`. A new pure add-on `trantor-files`, one module per
feature, taking basic-cli's `Path` (other path types convert at the call):

```roc
Walk.walk!, Walk.list!
Glob.matches, Glob.expand!
Temp.with_dir!, Temp.with_file!, Temp.create_dir!, Temp.create_file!  (+ _in)
Tree.copy!                    # { overwrite ?: Bool }
```

(User; both link operations stay in trantor-cli — moving `hard_link!` would
break basic-cli's `Path.hard_link!`.)

**Why:** single-primitive wrappers sit beside `write_bytes!`, `rename!`,
`hard_link!`; anything that walks or retries is a feature, and a module per
feature keeps names short without a catch-all `Files`.

**Rejected:** everything including append/copy/links in `trantor-files`;
functions generic over every path type.

### D-S2-21 Redirects take the raw resource

`File.Reader.descriptor` and `File.Writer.descriptor : _ -> Fs.Descriptor` in
trantor-cli; `trantor-process`'s `Stdio.Descriptor(Fs.Descriptor)` accepts
them. A `Writer` keeps the descriptor it was opened from. `FdHandoff` stays in
trantor-cli as an interface apps cannot import; `trantor-process`'s host
imports it. (User.)

**Rejected:** `trantor-process` converters over `File` internals; no `File`
integration.

### Test locations (amends D-S2-17)

trantor-cli: open-flag expects, `tests/fs-write-modes`, the `confined-race`
extension. `trantor-files`: glob corpus, `tests/walk-glob`, `tests/temp`,
`tests/copy` (tree; single-file copy and links in trantor-cli's
`fs-write-modes` or a `tests/links`). `trantor-process`: `ExitStatus` expects,
`tests/spawn`, and the three moved suites.

### D-S2-22 A confined symlink must point at something that exists inside the root

Under `fs-confined`, `symlink_at!` resolves the target from the link's own
directory beneath the root, following links, and creates the link only if that
finds an existing entry. Absolute, escaping and dangling targets fail.
Unconfined, the target is stored as given. (User: "Failing when the linked file
doesn't exist under the confined root seems like it'd avoid all the
link-related confinement problems.")

Found while extending `confined-race` (plan step 5): the link's location was
confined, its contents were not, and following an outside link was already
refused. Asked why the target was not resolved.

**Why:** refusing at follow time protects the confined app only. A link it
leaves pointing at `~/.ssh` is followed by the unconfined programs that later
touch the tree — a shell, an editor, a backup — which is the classic symlink
attack. A target that must exist inside the root cannot be planted for them.

**Cost:** a confined app cannot create a dangling link, and `Tree.copy!` under
confinement has to create links after the entries they point to.

**Not closed:** renaming a directory that holds relative links can still aim
them outside; the target can change between the check and the create.
*(Moving or hard-linking the link itself was closed after review: confined
`rename_at`/`link_at` re-check a symlink's target from its new location.)*

**Rejected:** storing contents unchecked (WASI's and cap-std's
`symlink_contents` behavior); a lexical check of the target text (passes
`sub/../..` spellings that a resolution catches, and allows dangling links);
refusing absolute targets only (cap-std's `symlink`; `../../..` still passes).

### D-S2-23 Streams on a descriptor share its cursor

Every stream from one descriptor, and any child handed that descriptor, share
the open file's single position, as Unix fds do. `write_via_stream!`'s `offset`
positions the cursor when the stream is created and nothing more; a reader
buffers ahead, so reading and writing one file independently takes two
`open_at!` descriptors. A non-seekable file (pipe, FIFO, `/dev/stdout`) skips
the seek. (User.)

Found in the second independent review: the first review's fix (shared cursor
for writes, so a child handed a writer's descriptor stops overwriting it) made a
write stream and a read stream on one descriptor interfere.

**Why:** descriptor handoff, the reason the position is shared, needs it;
`read_via_stream!` always shared it; the shim's `File.Reader` and `File.Writer`
each open their own descriptor.

**Rejected:** WASI's independent positions for every stream (handoff would need
a per-child reopen through `/dev/fd/N`, which duplicates rather than reopens on
macOS and repeats the path check under confinement); private positions synced
at handoff (the writer cannot know how far the child wrote, so it overwrites it
again).

### D-S2-24 A confined move re-checks a symlink only when the move can re-aim it

Under `fs-confined`, `rename_at!`/`link_at!` of a symlink are checked only when
its contents are relative and its parent directory changes (compared by path
text, so differently spelled parents are checked). Then the target must exist
inside the root from the new place, D-S2-22's creation rule. Absolute links and
same-directory renames always pass. A refusal is `PermissionDenied`, never
`NotFound`: the source exists. (User.)

Found in the second independent review: the first review's move check refused
every absolute and every dangling symlink, even renamed in place, with
misleading errors.

**Rejected:** refusing only escapes and allowing dangling targets (looser than
creation, and a dangling relative target could be created outside later);
removing the move check (the one-call re-aim D-S2-22 exists to stop).

### D-S2-25 Walk detects cycles by identity through WASI's `metadata-hash-at`

trantor-cli gains `Fs.metadata_hash_at! : Descriptor, List(U8), { follow_symlinks
: Bool } => Try({ lower : U64, upper : U64 }, [Io(IOErr)])`, a hash of device
and inode. `Walk` keeps the identities of the directories it is inside; a
followed link to one of them is reported `IsSymLink` and not descended. Two
links to the same non-ancestor directory are both walked (`find -L`). A
dangling followed link is `IsSymLink`; any other failure following a link fails
the walk, as an unreadable directory does. (User.)

Found in the second independent review: the lexical ancestor check missed
sibling-link cycles and path aliases (a walk that never finished, another with
262,142 entries) and skipped legitimate links, and every follow error was
swallowed.

**Why:** the only option that ends on every cycle with no false skips; the
primitive is WASI's and goes through the root policy like every other `*_at!`.
Skipping rather than failing keeps the rest of the tree, which is what a caller
asking to follow links wants.

**Rejected:** a cap on links followed per path (two links to `.` still fork to
about 2^40 entries within it); removing `follow_symlinks`.

### D-S2-26 SIGPIPE keeps its default; only the parent's write to a child's stdin suppresses it

Apps and children keep SIGPIPE's default disposition. `trantor-process`
suppresses it only around its own write to a child's stdin pipe: on macOS by
setting `F_SETNOSIGPIPE` on the pipe for the write and clearing it after, on
Linux by blocking SIGPIPE on the writing thread and consuming one the write
raised. (User.)

Found in the second independent review: the first review's fix left
`F_SETNOSIGPIPE` set, and the flag belongs to the open file, so a child handed
the stdin pipe through `ToStream` never got SIGPIPE (`yes` feeding
`head -c 1` exited 1 with "Broken pipe" instead of dying with 141).

The driver's `main` is a Rust `#[no_mangle] extern "C" fn main` in a staticlib
(trantor `src/codegen.rs`), so Rust's startup never sets SIGPIPE to ignored.

**Why:** trantor chose default SIGPIPE for its own CLI (`src/main.rs`
`restore_sigpipe_default`) so pipelines end quietly; apps and their children
should behave the same. The suppression covers the one write that must answer
`BrokenPipe` instead of killing the app.

**Cost:** a child writing the same pipe during the parent's write, as the
reader exits, gets one EPIPE instead of SIGPIPE. The Linux path is not run on
the development machine.

**Rejected:** ignoring SIGPIPE in the generated driver for every app (against
trantor's own choice; `app | head` would see `BrokenPipe`); clearing the flag
when the pipe is handed to a child (the parent's later writes can then kill the
app); ignoring it process-wide around each write (a global mutation that loses
another thread's signal).

### D-S2-27 Append is an open flag; `append_via_stream!` never changes the open file

`Fs.open_at!` gains `append ?: Bool` (`O_APPEND`, default `False`); `truncate`
or `directory` with `append` is `Unsupported`. `FsOps.open_append!`/`append!`
(behind `File.open_append!` and `Path.append_*`) open with `append: True`. On a
descriptor opened with it, `append_via_stream!` writes plainly and every write
is atomic at the end; on any other descriptor each write seeks the shared
cursor to the end first, which is not atomic against other appenders. Nothing
sets `O_APPEND` after open. (User.)

Found in the third independent review: `append_via_stream!` set `O_APPEND` on
the dup's open file, which the descriptor shares, so later `write_via_stream!`
offsets were ignored and a child handed the descriptor appended — for good.

**Why:** only `O_APPEND` gives the "including when other processes append"
promise `File.open_append!` makes, and setting it at open touches no shared
state. The flag departs from WASI's open flags, which D-S2-2 left open "until
something needs them"; this needs it.

**Rejected:** reopening the file by a path recorded at open (an open-by-name
race inside a descriptor API: a renamed or replaced file gets the appends or
the call fails); a seek to the end before every write everywhere (breaks the
multi-process promise).

### D-S2-28 Tree overwrite builds the replacement at `.copy-in-progress-<16 hex>`

`Tree.copy!` with `overwrite: True` makes each replacement file or link in the
destination's directory under `.copy-in-progress-` and 16 hex digits from the
OS entropy source, retried on `AlreadyExists`, then renames it over. A copy
that fails part way removes its partial file. (User, choosing the prefix.)

Found in the third independent review: the name `<to>.trantor-replace-<u64>`
added up to 37 bytes, so a destination name over about 218 bytes could be
copied but never overwritten.

**Why:** 34 bytes whatever the destination is called, one code path, and the
replacement-exists-before-the-old-one-goes guarantee for every name; a crash
leftover names what was happening.

**Rejected:** falling back to delete-then-create for long names (loses the
guarantee exactly there); shortening the destination name into the temporary
(UTF-8 boundaries, and long names sharing a prefix contend).

### D-S2-29 The generated driver opens closed standard fds on `/dev/null`

Before `roc_main`, trantor's generated driver `main` opens `/dev/null` for any
of fds 0, 1 and 2 the process started without, as Rust's startup does. (User.)

Found in the fourth independent review: the driver's `main` is exported to C
and skips Rust's startup, so in `app <&-` the next file opened took fd 0. A
spawn's close-on-exec pipe landed there, the child's `dup2(0, 0)` kept
close-on-exec, and exec closed the child's stdin; `collect!` returned `Ok` with
the input lost, and `Cmd.exec_output!` failed the same way.

**Why:** one fix at process start covers every component and every later
open; it is what basic-cli apps get from Rust's runtime.

**Rejected:** working around it at each spawn (every other component that
opens a file still takes the number).

*(Fifth review: the first version opened `/dev/null` through `std::fs`, which
adds `O_CLOEXEC`, so a child inheriting the refilled fd found it closed again.
It uses `open(2)` now.)*

### D-S2-30 A followed link out of a confined root is a link, not a failed walk

With `follow_symlinks: True`, a link whose target cap-std refuses
(`PermissionDenied`, how it reports an escape) is reported `IsSymLink` and not
descended, as a link that leads nowhere is. A real EACCES on a link's target is
treated the same. (User.)

Found in the fourth independent review: any tree holding an absolute link could
not be walked with follow under `fs-confined`, even at `max_depth: 1`.

**Rejected:** failing the walk (the confinement is working; the rest of the
tree is still walkable).

### D-S2-31 A seek-to-end append stream cannot be handed to a child

`append_via_stream!` on a descriptor opened without `append` records no fd, so
`FdHandoff.output_fd!` answers `NotAFile` and `Subprocess.spawn!` refuses it as
a redirect. (User.)

Found in the fourth independent review: the handed fd carries no `O_APPEND`, so
the child wrote at the shared cursor and overwrote the file the stream was
appending to (`0123456789` became `X123456789P`).

**Why:** the stream's promise is that every write lands at the end; a child
given its fd could not keep it. `File.open_append!`'s descriptor has `append`,
so redirecting to it works.

**Rejected:** documenting the mismatch (a redirect that silently corrupts).

### D-S2-32 `.` and `..` after a wildcard are not supported

A pattern with a `.` or `..` component after any component containing glob
syntax (`tree/*/../a.txt`, `*/.`) is `InvalidGlob`. `.` and `..` in the literal
leading directories (`../*.txt`, `a/./b/*`) keep working. (User: unsupported.)

Found in the sixth independent review: a walk never yields `.` or `..` entries,
so such a pattern silently matched nothing while `Glob.matches` accepted the
same path, and `*/../x` still walked every directory.

**Why an error:** "unsupported" as an empty result looks like "nothing there".

**Rejected:** expanding up to the wildcard and resolving the rest by name
(a second matching mode for a spelling globset does not support either).

### D-S2-33 A trailing `/` matches directories only

`tree/*/` and `tree/sub/` match only directories and links to directories, as
a shell does; results are spelled without the slash. `Glob.matches`, having no
filesystem, ignores the slash. (User.)

Found in the sixth independent review: the empty last component was dropped,
so `tree/*/` returned files and `tree/a.txt/` returned the file.

### D-S2-34 A literal name that cannot be checked is an error, not a non-match

The name check behind literal alternatives (`.`, `tree/..`, `/`, and the
fallback for a start that can be searched but not listed) treats only
`NotFound`, `NotADirectory` and a symlink loop as no match; any other error
fails the expansion. The fallback swallows only the walk's listing failure.
(User, choosing (a).)

Found in the sixth independent review: every error on a literal was no match,
so under `fs-confined` `../outside/d/f` silently found nothing while
`../outside/d/*` failed with `PermissionDenied`, and an I/O error on a literal
vanished.

**Why:** a dropped confinement refusal or I/O error is how a caller comes to
believe a file does not exist. The one silent refusal is the deliberate one in
D-S2-35. `IOErr` has no loop variant, so a loop is recognised by its OS error
number in `Other`'s message (62 on macOS, 40 on Linux, neither of which a path
lookup otherwise returns there).

**Supersedes** the fifth review's "any check error is no match".

**Rejected:** silent no match for literals (literal and wildcard spellings of
one escape disagree).

### D-S2-35 A glob start link out of a confined root is no match

*(Superseded by D-S2-37.)*

A start directory reached through a link whose listing cap-std refuses
(`PermissionDenied`) is no match, as D-S2-30 has the walk treat such a link. A
start spelled with `..` out of the root still fails. (User.)

Found in the sixth independent review: `outlink/*` failed under `fs-confined`
while a walk reported the same link as a link.

### D-S2-36 One copy for both backends, between opened handles; `fs-confined` holds against concurrent writers

`copy_file_at!` has one implementation. Each backend only opens: the source
(required to be a regular file) and the destination with `create_new`,
unconfined through `std::fs`, confined through cap-std. The bytes then move
handle to handle (`std::io::copy`); permission bits are set without
setuid/setgid; a partial destination is removed on failure. No timestamps, no
xattrs, one error order and one set of messages on both backends. No clone fast
path until something measures a need; one would be `fclonefileat` against a
cap-std parent directory handle with a single-component name. (User, choosing
(a).)

Found in the seventh independent review, which ran the APIs end to end:
unconfined `std::fs::copy` kept timestamps on APFS and dropped setuid, the
confined handle copy did neither, and the two refused a bad copy in a different
order with different messages. The split was accidental: step 4 used
`std::fs::copy` and cap-std's `Dir::copy`, and step 5 replaced only the
confined one after the race test found it escaping.

**The threat model, asked while deciding.** The user asked why the confined side
cannot check the destination is under the root and pass the path to
`std::fs::copy`, how an outside symlink would appear, and whether a process able
to coordinate that means the machine is already compromised. Answer recorded:
a check and a later use resolve the path twice, and a directory swapped for a
symlink in between escapes (the pre-cap-std root measured 192–970 of 20,000
reads outside; `Dir::copy`'s `fclonefileat` 108 of 2,000). The confined app
cannot make such a link itself (D-S2-22, D-S2-24; single-threaded; no
`trantor-process`), so the swap needs a second process. A same-user attacker
gains nothing from the race; one with less authority than the app does (a
trantor-cli tool run as root or a service in a user-writable tree, the shape of
CVE-2022-21658).

**Why:** `fs-confined` keeps resolving at the moment of use, so it holds against
lower-privileged concurrent writers, not only against the app itself; for copy
that costs the clone fast path.

**Rejected:** check the path is under the root, then use it through `std::fs`
everywhere (simpler, keeps `std::fs::copy`'s fast paths, and makes a privileged
confined tool escapable by a local user).

### D-S2-37 A glob start that reaches outside the root errors, through a link or `..`

D-S2-35's special case is dropped: a start link whose listing fails with
`PermissionDenied` fails the expansion like any other refused start. Only a
missing target, a file on the way, or a loop of links is no match. The walk's
D-S2-30 is unchanged: it reports such a link as a link rather than hiding a
refusal. (User, choosing (b).)

Found in the seventh independent review: nothing tells trantor-files which
backend it is on, and cap-std reports an escape as plain `PermissionDenied`,
so the rule also fired unconfined (`lz/*`, a link to a mode-000 directory,
silently matched nothing while the directory itself errored, and `lx/f` lost
its name-check fallback), and it missed a link one level into the start
(`outlink/sub/*` errored).

**Why:** D-S2-34's reason — a refusal reported as no match is how a caller
comes to believe a file does not exist — and the exception cannot be told from
an ordinary permission failure without new trantor-cli surface.

**Rejected:** `Fs.is_confined!` so the rule fires only when confined (a
primitive with no WASI counterpart that apps could branch on); a distinct
escape error from the confined backend (a string contract, and a changed error
kind for every confined caller).

### D-S2-38 A trailing `/` matches a link only if it resolves to a directory

When opening a link as a directory is refused, `Glob`'s `is_directory!` stats
the link with `metadata_hash_at!` (following). If that succeeds, the link
counts as a directory. If it fails too, the link is not a directory and the
pattern does not match. This refines D-S2-33. (User, accepting the
recommendation of no match.)

Found in the eighth independent review. Round 7 counted every
`PermissionDenied` as a directory. So `p2/*/` matched a link to a file
behind a mode-000 directory, and a link to nothing there. Under
`fs-confined`, `out/*/` matched links to `/etc/hosts` and to a missing
absolute path. bash's `echo p2/*/` lists only the real directory.

**Why:** opening a file or nothing as a directory fails as `NotADirectory` or
`NotFound` before any permission check (measured on macOS; Linux's `do_open`
checks the same way). A target that stats is therefore a directory that cannot
be read (`perm/lz` keeps matching). One that does not stat is not known to be a
directory. A match filter answering no match there agrees with D-S2-30's walk,
which reports such a link as a link.

**Rejected:** failing the expansion as D-S2-37 does for a refused start (a
start is where the search must go; an entry under a wildcard is only a
candidate).

### D-S2-39 `copy_dir_at!` makes a directory with its source's permission bits

`Fs.copy_dir_at! : Descriptor, List(U8), Descriptor, List(U8) => Try({},
[Io(IOErr)])` creates the destination directory, not its contents, and gives it
the source directory's mode. The owner always gets `0o700` so the copy can be
filled. Setuid, setgid and sticky are dropped, and the umask does not apply:
the host makes it `0o700` and then sets the mode on a no-follow handle. A
source that is not a directory, a link included, is `NotADirectory`, and an
existing destination is `AlreadyExists`. `Tree.copy!` makes every directory
through it. This amends D-S2-10, which had copy-tree use `create_dir_at!`.
(User: "copy_dir_at! host op".)

Found in the eighth independent review. With umask 022, a 0700 directory
holding a 0644 key copied as 0755, and other users could read the key, while
the docs said permission bits were copied.

**Why:** the raw stat record carries no mode bits (it is WASI-shaped), so only
a host op that reads the source can carry them, exactly as `copy_file_at!`
does for files. An exact mode cannot be set before the children are copied (a
0500 source blocks them), and D-S2-10 rejected a `set_permissions_at!`
primitive, so the owner's bits are the one deviation.

**Rejected:** `create_dir_at!` taking a mode (proposed first; Tree has no mode
to pass without adding `mode : U32` to the stat record); `mode` in the stat
record (Unix bits at the WASI-shaped layer); documenting that directories get
the umask (leaves the exposure).

### D-S2-40 `stat_at!` takes WASI's follow flag; executability follows links

`Fs.stat_at! : Descriptor, List(U8), { follow_symlinks : Bool }`, the same
flags record as `metadata_hash_at!`. `FsOps.executable!` (and so
`Path.is_executable!` and `Cmd.check_available!`) follows. Every other stat
caller passes `False`, unchanged. `Cmd`'s directory exclusion checks
`candidate/.`, so a link to a directory is still rejected. (User: "stat_at!
gets follow_symlinks".)

Found in the eighth independent review. `check_available!` judged a PATH
entry that is a symlink by the link's own mode bits:

- a dangling link was available where spawning answered `NotFound`;
- a link to a 0644 script was available where spawning answered
  `PermissionDenied`;
- on Linux, every link (always 0777) was available.

**Why:** `exec` and basic-cli's `file_is_executable` (`std::fs::metadata`)
both follow. WASI's `stat-at` takes path flags, so the flag is the WASI shape.

**Rejected:** resolving links in Roc with `readlink_at!` (path resolution
written again, loop limit included); following only in `Cmd` (leaving
`Path.is_executable!` answering for the link).

### D-S2-41 `FdHandoff` answers `Io` when a descriptor cannot be handed over

`output_fd!`, `input_fd!` and `descriptor_fd!` answer `Try(I32, [NotAFile,
Io(IOErr)])`. `Io` covers a failed `F_DUPFD_CLOEXEC` and a stream whose file
could not be cloned. For the second, sync-io-core's `Handoff` enum (`Fd`,
`NotAFile`, `Failed`) records the failure on the stream. `spawn!` returns the
first redirect's `Io` error as its own. This amends D-S2-14. (User, accepting
the recommendation.)

Found in the eighth independent review. Under `ulimit -n 5`, a `Descriptor`
redirect answered "has no file descriptor under it".

**Why:** a resource limit misreported as a wrong redirect sends the caller to
fix the wrong thing.

**Rejected:** rewording the one `NotAFile` message to cover both causes.

### D-S2-42 A name too long to look up leads nowhere, like a loop

`ENAMETOOLONG` is treated the way a loop of links is:
- a followed walk reports a link whose target name is too long as
  `IsSymLink`;
- a glob start or literal name that is too long is no match.

`FilesPath.leads_nowhere` covers both errors; neither has an `IOErr` variant,
so each arrives as `Other`. This amends D-S2-37's list of no-match failures.
(User, accepting the recommendation of no match.)

Found in the ninth independent review. Round 8 made every other `Other` on a
followed link fail the walk, so a link to a 300-byte name failed walks that
had reported it as a link. `long/toolong/*` errored where `loop/l1/*` was no
match. bash treats both as no match.

**Why:** like a loop, nothing refused anything: no such name can exist on this
system. D-S2-34's concern, a refusal read as an absence, does not apply.

**Rejected:** an error, as D-S2-37 gives a refused start.

### D-S2-43 `check_available!` asks the OS whether this user may execute

`Subprocess.can_execute! : OsStr => Try({}, [Io(IOErr)])` calls
`faccessat(X_OK, AT_EACCESS)`. It follows links, and resolves a relative path
against the userland cwd. On Unix, `Cmd.check_available!` uses it instead of
`Path.is_executable!`:
- `NotFound`, `NotADirectory` and `PermissionDenied` move on to the next PATH
  entry.
- Any other error ends the search on Linux, as glibc's `execvp` does, and
  moves on elsewhere.

`Path.is_executable!` keeps basic-cli's meaning, any execute bit (D-S2-40).
(User, accepting the recommendation.)

Found in the ninth independent review:
- `/usr/sbin/cupsd` (`r-x------ root`) was available, and spawning it
  answered `PermissionDenied`. A file on a `noexec` mount behaved the same.
- On Linux, a loop of links early in PATH was skipped by the check, while the
  spawn failed on it.

**Why:** `check_available!` exists to predict a spawn, and only the kernel's
access check sees effective ids, ACLs and mount flags. It lives in the
subprocess host, whose spawns are already unconfined, so it adds no
confinement surface.

**Rejected:** documenting the gap; changing `FsOps.executable!` to an access
check (it would change `Path.is_executable!`'s basic-cli meaning, and go
through the confined filesystem, which a spawn does not).

### D-S2-44 A mode at the raw layer, so temps are private

`Fs.open_at!` takes `mode ?: U32` (default `0o666`) and `Fs.create_dir_at!`
takes a mode (`Fs.default_dir_mode`, `0o777`, everywhere else); both are masked
by the umask, as `open(2)` and `mkdir(2)` are, and ignored on Windows.
`Temp` creates files `0o600` and directories `0o700`, as `mkstemp` and
`mkdtemp` do. This amends D-S2-10's "no `set_permissions_at!`" only for
creation, which needs no separate call. (User: "Follow Python+Rust".)

Found in the tenth independent review: temps followed the umask, so with
`umask 022` a temp file was `0644`. On Linux with `TMPDIR` unset that puts a
secret staged through `Temp.with_file!` in a world-readable `/tmp` file; macOS
is saved only by its per-user temp directory.

**Why:** an unguessable name protects creation, not what is written afterwards,
and every standard temp API creates privately. A mode at creation is what the
OS call already takes; D-S2-39's problem — `Tree.copy!` having no mode to pass,
since the stat record carries none — does not arise, because `Temp` knows the
mode it wants.

**Rejected:** documenting the umask behaviour (the exposure stays); a
`set_permissions_at!` primitive (D-S2-10's reason stands: a second call, and a
confinement surface, for something creation can do).

### D-S2-45 `SubprocessRaw`: the raw spawn is wired but not exported

`interfaces/subprocess-raw/` holds `spawn_redirected!`, the fd-numbered
`Redirect`, the `Handle` resource and the `Cmd` record. `Subprocess` aliases
`Cmd` and `Handle`, and `spawn!` is the only way an app starts a process. The
package exports `Subprocess` and `Cmd`, not `SubprocessRaw`, exactly as
trantor-cli wires `FdHandoff` without exporting it (D-S2-14, D-S2-21). (User,
accepting the recommendation.)

Found in the tenth independent review: `Redirect(Fd(I32))` and
`spawn_redirected!` were members of the exported `Subprocess`, so an app could
name any descriptor the process holds — another component's socket or log file
— as a child's stdout. Round 9 stopped the host closing such a number; this
stops an app naming one.

**Why:** the same reason `FdHandoff` is unimportable — an fd number is ambient
authority Roc cannot type. Moving the leaf costs nothing: `Stdio` names
resources, which is what apps use.

**Rejected:** leaving it as the raw layer beside `Fs`'s `*_at!` (those take
descriptors, which are resources, not numbers).

### D-S2-46 A wildcard searches through a link to a directory; `**` does not

`Glob.expand!` walks with `follow_symlinks: True`, so an ordinary wildcard or
literal component searches inside a linked directory — `src/*/main.roc` finds
one under a linked `src/vendor`. `**` neither matches a link's own component
nor reaches past one: the walk reports the depth of the shallowest link on each
path, and matching and pruning share one limit (`GlobPattern.Reach`, the index
from which `DoubleStar` may take nothing). Cycles end by identity (D-S2-25).
(User, accepting the recommendation.)

**This is zsh's rule, not bash's** — bash's `**` does match a link as the last
component it takes, and then follows it with ordinary components. The first
implementation (round 11) was wrong in both directions, and the twelfth review
measured all three: a pattern with an earlier `**` could not reach a linked
directory at all, while inside a link `**` consumed the link freely
(`a/**/*/y.txt` returned `a/lnk/deep/x/y.txt`, which neither shell gives).
`tests/glob-oracle` now compares expansions with zsh directly.

Found in the eleventh independent review: `src/*` found the link and
`src/*/main.roc` found nothing inside it, so the two disagreed about whether a
linked package directory existed. bash, zsh and Python's `glob` all follow
links for ordinary components and stop only at `**`.

**Why:** monorepo package links, `node_modules` links and `vendor` links are
common, and a glob that stops at them is surprising in exactly the case people
use one. Matching bash also keeps the one rule users can already state.

**Rejected:** recording the old behaviour and documenting it (the disagreement
between `src/*` and `src/*/main.roc` stays); following links for `**` too
(bash does not, and `**` through links can walk a tree many times over).

**Also amended here:** D-S2-40's "`Cmd`'s directory exclusion checks
`candidate/.`". That probe is gone: D-S2-43's `can_execute!` requires a regular
file, so a directory, a link to one and a FIFO are already refused, and
appending `/.` pushed a path near `PATH_MAX` over it — an executable at 1022
bytes read as missing while a spawn ran it (eleventh review).

### D-S2-47 Metadata follows a link, except where the kind is the question

`FsOps.size!` and `stat_field!` (`Path.size_in_bytes!`, the time accessors,
`is_readable!`, `is_writable!`) pass `follow_symlinks: True`, as
`executable!` has since D-S2-40. `kind!` — `Path.type!`, `is_file!`,
`is_dir!`, `is_sym_link!` — still reports the link. This amends D-S2-40's
"every other stat caller passes `False`, unchanged". (User: "follow".)

Found in the twelfth independent review: a symlink to a 100-byte file
reported 7 bytes (its target name's length), a link to a mode-000 file
reported readable, and a link to a 0444 file reported writable — while
`is_executable!` followed. basic-cli reads all of them through
`fs::metadata`, which follows.

**Why:** one rule for the whole group, and it is basic-cli's (P15). The kind
accessors are the documented exception, since asking what something *is* is
the one question a link answers for itself.

**Rejected:** following in none of them (`exec` follows, so
`check_available!` would be wrong again); leaving the split as it was.

### D-S2-48 `read_via_stream!` answers a result

`Fs.read_via_stream! : Descriptor => Try(Streams.InputStream, [Io(IOErr)])`,
as WASI's `read-via-stream` does and as `write_via_stream!` already did.
`IsADirectory` for a directory descriptor, and the clone's failure — no
descriptors left — is this call's error. `FdHandoff.descriptor_fd!` maps
`IsADirectory` back to `NotAFile`, keeping its documented answer.
(User: "error channel".)

Found in the twelfth independent review: under `ulimit -n 64`,
`File.open_reader!` answered `Ok` and the reader's first read failed with
EMFILE, blaming the read for the open's failure — the same misattribution
D-S2-41 fixed on the handoff.

**Why:** a stream that owns a duplicate can fail to be made, so the leaf that
makes it needs somewhere to say so.

**Rejected:** keeping the erroring-stream (it is still how a stream reports a
failure it cannot return, but it is no longer how `open_reader!` learns of
one).

### D-S2-49 A linked directory that cannot be listed is no match

In `Glob.expand!`'s search (`Walk`'s `skip_unreadable_links`), a directory
reached through a symlink whose listing is refused contributes nothing
instead of failing the expansion. A real directory that cannot be listed
still fails it (D-S2-34). (User: "no match".)

Found in the twelfth independent review, as a regression from D-S2-46:
before links were followed, such an entry could not be reached at all, so one
unreadable link anywhere under the search turned a working glob into
`PermissionDenied`. bash and zsh return the other matches.

**Why:** D-S2-38's line — a start is where the search must go, an entry under
a wildcard is only a candidate — now applies to links the search steps
through as well.

**Rejected:** failing, as for a real directory (a link the app never named
decides the whole expansion); skipping refusals everywhere (D-S2-34's reason
stands: a refusal is not an absence).

## Still open

- Named preopens (`Fs.preopens!` returning names, WASI's shape) — D-S2-7
  left them for when a confined world needs a temp dir without configuration.
- Signal handling in the parent (SIGINT/SIGTERM as events) is not in this step.

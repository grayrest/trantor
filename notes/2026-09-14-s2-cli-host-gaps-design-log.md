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

## Still open

- Named preopens (`Fs.preopens!` returning names, WASI's shape) — D-S2-7
  left them for when a confined world needs a temp dir without configuration.
- Signal handling in the parent (SIGINT/SIGTERM as events) is not in this step.

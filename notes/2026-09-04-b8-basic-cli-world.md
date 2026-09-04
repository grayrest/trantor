# B8 — the `roc:basic-cli` world, the migration proof, the confinement swap

Gate B8 of [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md).
Fixture: `tests/golden/b8-basic-cli/` (`verify.sh`). P14 (all-in-one world),
P15 (basic-cli's own `Path`/`File`/`Env` verbatim — decided mid-gate, see the
design log), D10/D11 (publish + tier), P8 (confinement as a swappable impl).

## Result ✅

- **Migration proof: 28/28** non-sqlite basic-cli **0.21.0** examples (from the
  release tag of the basic-cli repo) `roc check` against the composed world
  with **only the platform URL changed**. Measured honestly: before P15 it was
  **1/29** (`hello-world`); the repo's HEAD examples score 27/29 because two of
  them already use a post-0.21 `PathErr(err, path)` payload — they fail against
  the real 0.21 platform too (checked).
- **21 examples run** with basic-cli's output: argv (incl. argv[0]), stdin
  (line + read-to-end + bytes), env vars, file read/write/replace/size/
  permissions, buffered line reader, directory create/list/delete, subprocess
  (`echo`, `/usr/bin/env -i`, exit codes), url, random, time, locale.
  Interactive (`tty`, snake) and network (`tcp-client`, `http-*`) examples
  check but aren't run; `check-command` (PATH search) postdates the tag and
  ran by hand from HEAD.
- **Publish:** `hematite publish` → `dist/platform/*.roc` + `targets/arm64mac/
  *.a` + `baseline.lock` (`abi_fingerprint`); the test-only `testnet` archive
  is not in it. `hematite tier extension/` → **Tier 1** for a pure-Roc module.
- **Confinement swap:** `world-confined.toml` differs from `world.toml` in one
  wiring line (`fs = "fs-confined"`). The same escaping app prints
  `escape: allowed` on the baseline and `escape: denied` on the confined
  world; in-cwd writes work on both. seahaven-as-a-component, delivered.

## What the world is

basic-cli 0.21's `exposes` minus `Sqlite`, plus `Udp`, `Sockets`, `Streams`,
`Temporal`, and P11's packages as `StrPath`/`OsPath`. **18 modules
byte-verbatim** from basic-cli (`Stdout`, `Stderr`, `Stdin`, `Tty`, `Utc`,
`Sleep`, `Random`, `Locale`, `Url`, `InternalDateTime`, `Tcp`, `Http`,
`InternalHttp`, `OsStr`, **`Path`, `File`, `Env`**) or seahaven (`OsStr`
identical); seahaven's `Cmd.roc` carries a 4-line bridge (two `Path`
constructors basic-cli's `Path` lacks). 78 hosted symbols across 12 host
archives; the shim is ~300 lines of pure Roc.

## Host-side changes this gate forced

- **argv[0].** B2's `cli-host` skipped the program name; basic-cli's `main!`
  (and WASI `get-arguments`) include it. Every arg-taking example silently
  saw no args until this was fixed — the kind of contract detail only a real
  app catches.
- **`dir_list!` joins entries to the directory** (`std::fs::read_dir`'s
  `entry.path()`); `FsOps.list!` yields bare names, so the shim joins.
- **`Streams.write!` flushes after every write.** The process exits through
  Roc's runtime, not Rust's `main`, so Rust's line-buffered stdout silently
  dropped a trailing partial line (`bytes-stdin-stdout` printed nothing).
  basic-cli flushes per write; now so does the stream primitive.
- **`roc:sync-io/read_until!`** — buffered delimiter read; `Input` became a
  `BufReader`, so files, stdin and sockets get line reads from one primitive.
  `File.Reader` is an alias of `InputStream`. B1's `[sync-io] read N bytes`
  gauge line is gone (a front door can't print to stderr).
- **`stat_at!` gained `accessed_ns`/`created_ns`** (all three as `U128`,
  basic-cli's type); `fs-core` aliases the record's glue name.
- **Driver contract** is basic-cli's `main! : List(NativeOsStr)`; the
  adapter builds argv from `CliEnv` count/at as `Utf8`.
- **`Env.set_cwd!` propagates to subprocesses** (added post-review, cwd-model
  Option A). The port keeps a single userland cwd in the `cell` (what
  `FsOps.set_cwd!` writes and file ops resolve against); the subprocess host
  now reads that cell (`hematite__cell__get`) and sets `Command.current_dir`
  before spawning, so a child runs in the same directory files resolve
  against — basic-cli's observable single-cwd behavior — without mutating this
  process's real cwd (stays capability-clean and projects to WASI). Empty cell
  = inherit the process cwd. `cwd-app` asserts `pwd` reports `/usr` after
  `set_cwd!("/usr")` while the process cwd is unchanged.

## Findings

1. **Measure the migration claim before trusting the plan's wording.** The
   plan said "a real example runs by changing only its URL"; one did. The
   1/29 → 28/28 delta came from three things the plan's P11 had reasoned
   away: argv type, basic-cli's OsStr `Path`, and `File.Reader`.
2. `Path.type!` renders `IsFile`/`IsDir` (basic-cli's tags), not
   `File`/`Dir` (the `Host.PathType` tags) — the verbatim module maps them.
3. Roc: `;` is not a statement separator (multi-line blocks only); a block
   appended after a nominal type's closing brace lands at top level and its
   errors read as "no associated fn", not a parse error.
4. `git show tag:examples/x.roc` in zsh: `$T:examples` triggers the `:e`
   history modifier — quote as `"${T}:${f}"`.

## Exit ✅

`verify.sh`: 28/28 check, 22 exact/substring run assertions, publish
artifacts + Tier 1, confinement swap (allowed → denied), default world left
composed.

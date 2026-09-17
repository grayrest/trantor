# Trantor

A system for building Roc platforms by composition.

By default Roc's access to the outside world is mediated through its platform.
This has some benefits but a drawback is that a Roc app can only have one platform
and that one platform must provide *all* services the app will need over its
lifetime. Needing something the platform didn't anticipate means a Roc developer
would need to learn Rust/Zig/Go and fork the platform, which seems like a tall
ask.

Trantor is a system for building a platform from a composition of Rust/Roc parts:

- **Interfaces** are the Roc API an app sees, declared without bodies (abstract).
- **Components** implement interfaces, either in Rust (a *host* component) or in
  Roc.
- **Driver** is the one component that owns `main`.

A **world** (`world.toml`) names the interfaces, the components and which
component implements each interface. The `trantor` binary turns that into an
ordinary Roc platform that stock `roc build` compiles: the platform's `main.roc`,
the binding modules, a cargo workspace for the host components, and the
generated ABI crate they link against.

The API an app sees is set by the interface, not by what implements it, so
the implementation can change without the app changing. These are intended
to be small enough that replacing one when different behavior is needed is
a reasonable amount of work.

A set of related components are grouped together as a **package**. The idea is
to start from a baseline package providing the Driver and add packages to it.
The currently released set of packages are written around a CLI baseline and
intended to function as a standard library. A second baseline around building
GUI applications is mostly built but unreleased and a third baseline around
network services is planned.

| Package | Domain |
|---|---|
| [trantor-cli](https://github.com/grayrest/trantor-cli) | Baseline: a Roc port of WASI 0.3's `wasi:cli`, plus a `basic-cli` 0.21 shim |
| [trantor-files](https://github.com/grayrest/trantor-files) | Walking directories, globs, temporary files, copying trees |
| [trantor-process](https://github.com/grayrest/trantor-process) | Starting and controlling child processes |
| [trantor-net](https://github.com/grayrest/trantor-net) | Sockets and a blocking HTTP client |
| [trantor-terminal](https://github.com/grayrest/trantor-terminal) | Terminal handling, raw modek, key input, and ANSI escapes|
| [trantor-temporal](https://github.com/grayrest/trantor-temporal) | TC39 `Temporal`-shaped calendar and time-zone arithmetic |
| [trantor-hash](https://github.com/grayrest/trantor-hash) | Structural hashing of encodable values |
| [trantor-encoding](https://github.com/grayrest/trantor-encoding) | Base64, hex, CSV and TOML |
| [trantor-random](https://github.com/grayrest/trantor-random) | PRNG generators and UUIDs |

Packages are intended to rely on the least amount of authority that's
reasonable. An app without `trantor-process` can't start processes and
one without `trantor-net` can't open a socket or make a network request.

## Building the `trantor` binary

- Rust (stable; `rust-toolchain.toml` pins the channel).
- A Roc compiler, found at `$ROC`, then `roc` on `$PATH`, then `~/.bin/roc`.
- The `RustGlue.roc` spec that matches that compiler, found at `$GLUE`, then
  `~/.bin/RustGlue.roc`, then `~/.roc/glue/RustGlue.roc`.
- LLVM's `nm` for the symbol-collision scan, at `$LLVM_BIN` (default
  `/opt/homebrew/opt/llvm/bin`).

```bash
cargo build --release
./target/release/trantor --help # lists the commands.
```


## Example: Growing a platform

Say your app needs a service. For this example it'll be capitalizing a
string, but the path is the same for resizing images, talking to a database,
or anything else a Rust library does well. We'll take it in steps:

1. **Shell out** There are CLI utilities for many tasks, matches what you'd
   do in `basic-cli`.
2. **Trantor package** Not demonstrated in this example but a goal for this
   project is to provide an ecosystem for common app needs. Hopefully one
   without a capitalization or left-pad service.
3. **Move it into a Rust package** When you want more control or don't want
   to depend on a utility being installed on the machine. The Rust ecosystem 
   is large enough to cover most needs.
4. **Move it into Roc** Optimized Roc can match the host languages for speed
   so the long term dream is to move dependencies into Roc.

For this example the app is written once, before step 1, and isn't edited again.

### The project

```bash
trantor new shout --from ../trantor-cli
cd shout
trantor new-interface . upper
```

`trantor new` writes `world.toml`, `app/main.roc`, a cargo workspace for
components and a `.gitignore`, then composes once so the project builds
from the start. `trantor new-interface` adds an interface and a host
component that implements it, and wires them together with the `world.toml`.
Step 1 starts a child process, so add `trantor-process` to `[deps]` as well:

```toml
# world.toml
[world]
name = "shout"
exports = ["Upper"]

[deps]
base = { path = "../trantor-cli" }
trantor-process = { path = "../trantor-process" }

[interfaces.upper]

[components.upper-host]
kind = "host"
lang = "rust"
exports = ["upper"]

[wiring]
upper = "upper-host"
```

For the interface declaration:

```roc
# interfaces/upper/Upper.roc
Upper :: [].{
	shout! : Str => {}
}
```

```toml
# interfaces/upper/interface.toml
module = "Upper"

[[hosted]]
leaf = "shout!"
symbol_stem = "shout"
```

The app consumes `Upper`:

```roc
# src/main.roc
app [main!] { pf: platform "../target/trantor/shout/platform/main.roc" }

import pf.IOErr
import pf.OsStr
import pf.Upper

main! : List(OsStr) => Try({}, [Io(IOErr), ..])
main! = |_args| {
	Upper.shout!("resize me")
	Ok({})
}
```

### Step 1: shell out

Shell out from Roc using `trantor-process`'s `Cmd`:

```roc
# components/upper-shell/Upper.roc
import Cmd
import Stdout

Upper :: [].{
	shout! : Str => {}
	shout! = |words| {
		_ = shouted!(words)
		{}
	}
}
shouted! = |words| {
	child = Cmd.new_str("tr").arg_str("a-z").arg_str("A-Z").spawn!({ stdin: Pipe, stdout: Pipe })?
	out = child.collect!(Str.to_utf8(words))?
	Stdout.line!(Str.from_utf8_lossy(out.stdout))
}
```

Declare it and point the wiring at it:

```toml
# world.toml
[components.upper-shell]
kind = "roc"
exports = ["upper"]

[wiring]
upper = "upper-shell"
```

```bash
trantor run .    # RESIZE ME
```

### Step 3: Rust package

This is expected to generally be wrapping pre-written crates and the example
will do it as an ordinary cargo workspace in `components/upper-host`.

We'll need to change the wiring by deleting the `[components.upper-shell]` entry,
and dropping the `trantor-process` from `[deps]`.

```toml
# world.toml
[deps]
base = { path = "../trantor-cli" }
## DELETE
## trantor-process = { path = "../trantor-process" }
##
## [components.upper-shell]
## kind = "roc"
## exports = ["upper"]

[wiring]
upper = "upper-host"
```

Ask Trantor for the Rust signature instead of writing it by hand:

```bash
trantor build . --platform-only
trantor interface-stub . upper
```

```rust
/// Roc: `Upper.shout! : Str => {}`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__upper_host__shout(arg0: RocStr) {
    // Owned argument (B0): released once, after the last read of it.
    unsafe { arg0.decref(abi::host()); }
    todo!()
}
```

Then fill it in:

```rust
use convert_case::{Case, Casing};

pub extern "C-unwind" fn trantor__upper_host__shout(arg0: RocStr) {
    let loud = arg0.as_str().to_uppercase();
    unsafe { arg0.decref(abi::host()); }
    println!("{loud}");
}
```

```bash
trantor run .    # RESIZE ME, with no subprocess
```

No change to the app.

### Step 4: Roc

Capitalizing is a pure function and pure fuctions belong in Roc! Alas,
we lose unicode support in the process.

```roc
# components/upper-roc/Upper.roc
import Stdout

Upper :: [].{
	shout! : Str => {}
	shout! = |words| {
		_ = Stdout.line!(Str.with_ascii_uppercased(words))
		{}
	}
}
```

Declare it in place of the host component and point the wiring at it:

```toml
# world.toml
[components.upper-roc]
kind = "roc"
exports = ["upper"]

[wiring]
upper = "upper-roc"
```

```bash
trantor run .    # RESIZE ME, in Roc
```

Still no change to the app through the whole process.

## Testing

### Testing an app

```bash
trantor test .
```

In a directory with `world.toml`, this does three things:

1. Composes the world.
2. Runs `roc test` over the app's `expect`s.
3. Runs `cargo test` over your own crates, if the workspace has any.

`trantor check .` performs a compose and typecheck without a build.

### Testing a package

In a directory with `package.toml`, `trantor test .` treats the package the way
a consumer would:

1. **Composes it alone.** An add-on package must fail here, naming the missing
   driver; a baseline must succeed.
2. **Composes it with its `[dev-deps]`.** This is the baseline it's tested
   against, and it never reaches consumers.
3. **Checks the dev-deps alone** don't already export the package's modules, so
   the tests exercise this package and not something beneath it.
4. **Runs the package's `expect`s.**
5. **Runs each `tests/<name>/`**, which is one of:

| Contents | What runs |
|---|---|
| `main.roc` + `expected` | An app, its stdout compared line by line |
| `Cargo.toml` | `cargo test --release` |
| `test.sh` | A script, given `TRANTOR`, `ROC`, `PKG`, `TMP`, `DEPS` and `DEV_DEPS` |

Use `test.sh` for anything a stdout diff can't express:

- exit codes and signals;
- timing and deadlines;
- races;
- a peer process such as a test server;
- comparing against an oracle.

The current trantor packages lean on scripts. A few examples:

- [trantor-files' glob tests](https://github.com/grayrest/trantor-files/tree/main/tests/glob-oracle)
  compare every pattern's matches against zsh's.
- [trantor-cli's path tests](https://github.com/grayrest/trantor-cli/tree/main/tests/path-spellings)
  run every filesystem operation over every spelling of a path on both the
  confined and unconfined filesystems, and require the two to agree.
- [trantor-terminal's pty rows](https://github.com/grayrest/trantor-terminal/tree/main/tests/pty)
  drive the app through a real pseudo-terminal. Each has a mutation control that
  breaks the code and checks that the row notices.

### Testing the trantor binary

```bash
cargo test --release        # unit tests
cargo clippy --release
just verify                 # every golden fixture
just verify u1              # the fixtures whose path contains "u1"
```

Each directory in `tests/golden/` is one end-to-end fixture, and its `verify.sh`
builds trantor and runs it on a real project:

- `u1-front-door` is the walkthrough above.
- `b8-basic-cli` builds basic-cli's examples against trantor-cli's shim.
- `roc-shim` implements the filesystem in Roc behind the same `Path` module a
  host implementation uses.

`just verify` writes each fixture's full output to `target/verify-logs/`. It
fails if any fixture fails, and also if a fixture leaves the git tree dirty. A
fixture that needs a sibling checkout (such as `../trantor-cli`) prints `SKIP`,
and the runner reports it as skipped, not passed.

## Design records

- `plans/`: what each piece of work set out to build, and notes on how it went.
- `notes/`: the decisions behind it, as numbered decision records with the
  alternatives that were rejected.

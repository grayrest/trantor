# trantor

A build tool that composes Roc platforms out of components.

A Roc app talks to the outside world through its platform, and a platform is
normally one hand-written unit: its Roc API, the Rust host behind it and the
glue between them. Using a Rust library the platform didn't anticipate means
forking the platform. trantor splits the platform into parts:

- **Interfaces** are the Roc API an app sees, declared without bodies.
- **Components** implement interfaces, either in Rust (a *host* component) or in
  Roc.
- **The driver** is the one component that owns `main`.

A **world** (`world.toml`) names the interfaces, the components and which
component implements each interface. `trantor` turns that into an ordinary Roc
platform that stock `roc build` compiles: the platform's `main.roc`, the binding
modules, a cargo workspace for the host components, and the generated ABI crate
they link against.

The API an app sees is set by the interface, not by what implements it, so
the implementation can change without the app changing. That's what the
walkthrough below shows.

Most apps start from a baseline package and add packages to it:

| Package | What it gives an app |
|---|---|
| [trantor-cli](https://github.com/grayrest/trantor-cli) | The baseline: a Roc port of WASI 0.3's `wasi:cli`, plus a `basic-cli` 0.21 shim |
| [trantor-files](https://github.com/grayrest/trantor-files) | Walking directories, globs, temporary files, copying trees |
| [trantor-process](https://github.com/grayrest/trantor-process) | Starting child processes |
| [trantor-net](https://github.com/grayrest/trantor-net) | Sockets and a blocking HTTP client |
| [trantor-terminal](https://github.com/grayrest/trantor-terminal) | Raw mode, key input and screen drawing |
| [trantor-temporal](https://github.com/grayrest/trantor-temporal) | `Temporal`-shaped calendar and time-zone arithmetic |
| [trantor-hash](https://github.com/grayrest/trantor-hash) | Structural hashing of encodable values |
| [trantor-encoding](https://github.com/grayrest/trantor-encoding) | Base64, hex, CSV and TOML |
| [trantor-random](https://github.com/grayrest/trantor-random) | Seeded generators and UUIDs |

A package that can't do something doesn't give an app that power: an app
without trantor-process can't start processes, and one without trantor-net
can't open a socket.

## Requirements

- macOS.
- Rust (stable; `rust-toolchain.toml` pins the channel).
- A Roc compiler, found at `$ROC`, then `roc` on `$PATH`, then `~/.bin/roc`.
- The `RustGlue.roc` spec that matches that compiler, found at `$GLUE`, then
  `~/.bin/RustGlue.roc`, then `~/.roc/glue/RustGlue.roc`.
- LLVM's `nm` for the symbol-collision scan, at `$LLVM_BIN` (default
  `/opt/homebrew/opt/llvm/bin`).

```bash
cargo build --release
```

`target/release/trantor --help` lists the commands.

## Moving a capability without touching the app

Say your app needs a text service. For this walkthrough it's capitalizing a
string, but the path is the same for resizing images or anything else a Rust
library does well. You'll usually take it in three steps:

1. **Shell out.** A tool on the machine already does the job, so call it.
2. **Move it into a Rust package** when you'd rather not depend on the tool
   being installed: `cargo add` a crate and call it in-process.
3. **Move it into Roc** once the work needs no host at all, so there's no Rust
   left to build.

The app is written once, before step 1, and isn't edited again.

### The project

```bash
trantor new shout --from ../trantor-cli
cd shout
trantor new-interface . upper
```

`new` writes `world.toml`, `app/main.roc`, a cargo workspace for your components
and a `.gitignore`, then composes once so the project builds from the start.
`new-interface` adds an interface and a host component that implements it, and
wires them together:

```toml
# world.toml
[world]
name = "shout"
exports = ["Upper"]

[deps]
base = { path = "../trantor-cli" }

[interfaces.upper]

[components.upper-host]
kind = "host"
lang = "rust"
exports = ["upper"]

[wiring]
upper = "upper-host"
```

Declare what the app can call in `interfaces/upper/Upper.roc`, and name the
matching hosted function in `interfaces/upper/interface.toml`:

```roc
Upper :: [].{
	shout! : Str => {}
}
```

```toml
module = "Upper"

[[hosted]]
leaf = "shout!"
symbol_stem = "shout"
```

The app only ever sees `Upper`:

```roc
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

Ask trantor for the Rust signature instead of writing it by hand:

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

Fill in the body with a call to `tr`:

```rust
pub extern "C-unwind" fn trantor__upper_host__shout(arg0: RocStr) {
    let s = arg0.as_str().to_string();
    unsafe { arg0.decref(abi::host()); }
    let out = std::process::Command::new("tr").arg("a-z").arg("A-Z")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
        .spawn().and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(s.as_bytes())?;
            c.wait_with_output()
        }).expect("tr");
    println!("{}", String::from_utf8_lossy(&out.stdout));
}
```

```bash
trantor run .    # RESIZE ME
```

### Step 2: a Rust package

`components/upper-host` is an ordinary crate in an ordinary cargo workspace, so
cargo and rust-analyzer work on it as usual:

```bash
cargo add convert_case --manifest-path components/upper-host/Cargo.toml
```

```rust
use convert_case::{Case, Casing};

pub extern "C-unwind" fn trantor__upper_host__shout(arg0: RocStr) {
    let loud = arg0.as_str().to_case(Case::Upper);
    unsafe { arg0.decref(abi::host()); }
    println!("{loud}");
}
```

```bash
trantor run .    # RESIZE ME, with no subprocess
```

The signature didn't change and neither did the app.

### Step 3: Roc

Capitalizing needs no host, so the Rust can go. Write a Roc component that
implements the same interface, printing through trantor-cli's `Stdout` as the
Rust versions print with `println!`:

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
[components.upper-roc]
kind = "roc"
exports = ["upper"]

[wiring]
upper = "upper-roc"
```

Then delete `components/upper-host` and remove it from `members` in
`Cargo.toml`:

```bash
trantor run .    # RESIZE ME, with no Rust of your own
```

The app still calls `Upper.shout!`, and its source is the same file, byte for
byte, in all three steps.

The walkthrough also runs as a test,
[`tests/golden/u1-front-door/verify.sh`](tests/golden/u1-front-door/verify.sh).
It runs all three steps and fails if any of these checks don't hold:

- the output is the same in all three steps;
- `app/main.roc` has the same hash throughout;
- step 2 really calls the crate;
- step 3's binary no longer contains the host symbol.

To stay offline, the test swaps in two stand-ins:

- **Crate:** a local crate instead of one from crates.io.
- **Baseline:** a minimal one instead of trantor-cli. It provides the driver and
  a one-line `Stdout`, which its Roc component calls directly.

## Testing

### Your app or world

```bash
trantor test .
```

In a directory with `world.toml`, this does three things:

1. Composes the world.
2. Runs `roc test` over the app's `expect`s.
3. Runs `cargo test` over your own crates, if the workspace has any.

`trantor check .` is the faster inner loop: compose and typecheck, no build.

### A package

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

The trantor packages lean on scripts. A few examples:

- [trantor-files' glob tests](https://github.com/grayrest/trantor-files/tree/main/tests/glob-oracle)
  compare every pattern's matches against zsh's.
- [trantor-cli's path tests](https://github.com/grayrest/trantor-cli/tree/main/tests/path-spellings)
  run every filesystem operation over every spelling of a path on both the
  confined and unconfined filesystems, and require the two to agree.
- [trantor-terminal's pty rows](https://github.com/grayrest/trantor-terminal/tree/main/tests/pty)
  drive the app through a real pseudo-terminal. Each has a mutation control that
  breaks the code and checks that the row notices.

README examples aren't checked automatically. A package that wants its examples
checked keeps a suite for them.

### trantor itself

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

Package decisions are recorded here too. For example, `D-S2-*` covers
trantor-cli's host gaps, trantor-files and trantor-process.

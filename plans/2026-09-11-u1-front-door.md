# Plan: U1 — the front door (project setup, dependencies, new interfaces)

> **Status: NOT STARTED. Revised 2026-09-11** after an independent review that
> measured the tree rather than the prose; findings and the decisions they
> forced are in the design log's "Independent review" and "Decisions from the
> review" sections. **Scope halved: prebuilt baselines are deferred (D-U1-9).**
> Design log:
> [`notes/2026-09-11-u1-front-door-design-log.md`](../notes/2026-09-11-u1-front-door-design-log.md)
> (D-U1-1…15). Toolchain pinned at `~/.bin/roc` = `roc-b07d7e-rebased-main`,
> `RustGlue-b07d7e-rebased-main.roc`. macOS only, unchanged by this pass.

One repo changes: **trantor**.

## Scope

**In (Track A) — dependencies and project setup, all source-composed.** A
dependency is one line, fetched from GitHub or a local path and composed from
source. A project authors no driver, no interfaces directory and no components
directory. `cargo add` works. A new interface's Rust signature is generated
rather than guessed.

**Out (Track B) — prebuilt baselines, deferred to a second publisher
(D-U1-9).** The portable fingerprint, release assets, and Tier-1 consumption.
The design is settled and recorded; only the timing moved. Track A does not
depend on any of it, and the deferred surface is disarmed rather than left
lying (P0).

**Dropped: the "no Rust toolchain" claim.** Composing from source needs cargo,
which is what every Rust project needs. Say the true thing.

**Dropped: old exit criterion 7.** Already implemented — `scan.rs:225-236`
names both components, the symbol and `shared_symbols`, and
`nm-scan/verify.sh:26-27` already asserts all three.

## Exit (the whole plan)

Gated by `tests/golden/u1-front-door/verify.sh` and a new aggregate runner.
Every check prints the size of what it examined and fails when that is zero.

1. **Every command in the walkthrough exits zero and prints no error text**
   (D-U1-15, replacing the old file-count metric). The gate captures stdout and
   stderr per command and fails on a non-zero exit or a non-empty stderr. It
   also records the `world.toml` diff the user hand-writes at each step, and
   fails if any step requires editing a file the walkthrough did not name.
2. `cargo add image` into a component scaffolded by `trantor new-interface`
   **exits zero on a project fresh from `trantor new`**, with no prior compose.
   `cargo metadata` on that component also exits zero — that is the
   rust-analyzer precondition, and it is the half that actually gets used.
3. `trantor add <org>/<repo>` resolves a semver tag, writes `trantor.lock` with
   a peeled commit sha, and composes. A second `add` is a no-op changing no
   byte of the lock. `trantor update` is what moves a pin, and the gate asserts
   it moves exactly the one named.
4. A `{ path = … }` dep composes with the network unreachable. The gate runs it
   with no network and fails if anything resolves by reaching out.
5. `trantor interface-stub resize` emits Rust that compiles unmodified, **links
   into the composed world, and runs the app clean under the alloc gauge** —
   `TRANTOR_ALLOC_GAUGE=1` reporting `allocs > 0` and `live == 0`, the
   `b8-basic-cli/verify.sh` `balance()` shape. Compiling is the precondition,
   not the gate: a `todo!()` body compiles, and the owned-argument rule
   (D-U1-7) is a runtime property.
6. Two deps colliding on a Roc module name, an archive name after `sanitize`,
   a `[packages]` alias, or one wiring key each produce an error naming **both
   packages**. Four cases, four fixtures.
7. A dep whose manifest declares `cargo_root = "../../.."` (or an absolute
   `path`, or an escaping `interfaces_dir`) is rejected before cargo runs.
8. `trantor --help` lists every subcommand; `trantor` with no args does the
   same and exits non-zero.
9. Aggregate runner green across all 20 fixtures, and `git status --porcelain`
   **empty afterwards** — the check that fails when P5's `.gitignore` prune is
   wrong. Zero-warning build, clippy clean.

## Manifest additions

`package.toml` — new, one per publishing repo, the offer (D-U1-2):

```toml
[package]
name = "resize"
version = "0.2.0"            # informational; the resolved tag is the truth

[provides.resize]
module = "Resize"
component = "resize-host"

[components.resize-host]
kind = "host"
path = "."                   # confined to the package root (D-U1-13)

[deps]
io = { path = "../roc-io" }  # or { github = "karl/roc-io" }
```

A baseline additionally declares what a consumer inherits (D-U1-6):

```toml
[package]
name = "basic-cli"
provides_driver = "main-driver"
```

`world.toml` — one new section; `driver`, `components` and `wiring` become
optional (they are required today, `manifest.rs:15-26`, which is why the
four-line manifest in the previous revision of this plan did not parse):

```toml
[world]
name = "resize"
# driver = …                 # OPTIONAL when exactly one dep provides one

[deps]
basic-cli = { github = "karl/roc-basic-cli" }
resize    = { path = "../roc-resize" }
```

`trantor.lock` — generated, committed. No `baseline` field; that was Track B.

```toml
# generated by trantor; run `trantor update` to move a pin
[[package]]
name = "basic-cli"
github = "karl/roc-basic-cli"
tag = "v1.2.0"               # or branch = "main" when the repo has no tags
commit = "a1b2c3d4…"         # the PEELED sha (see P3)
```

## Phases

Ordered by value-per-risk, not by dependency: P1 ships alone and is useful on
its own, P2 unblocks the only Rust story left, and the manifest surgery that
everything else needs comes after both.

### P0 — disarm the deferred surface (D-U1-10)

Small, and it stops two commands emitting confident wrong answers for however
long Track B waits.

- `abi_fingerprint`: hard error when roc or the glue spec is unobtainable.
  Today both failures are swallowed and the function returns the bare FNV
  offset basis `cbf29ce484222325`, identical on every machine (measured).
- `classify`: resolve the world and fail loudly rather than reading an empty
  component map as purity. `trantor tier tests/golden/b8-basic-cli/extension`
  prints Tier 1 today on a world `compose` refuses — the fifth instance of the
  silent-check pattern. Print the component count examined.
- Record, do not fix, the `test_only`/`dist` inconsistency: the published
  `main.roc` lists `libtestnet_host.a` in `inputs` and binds its hosted symbol
  while `publish` deletes the archive. Fixing it needs a Track B decision.

### P1 — `interface-stub` (independent; ship first)

`trantor interface-stub <interface>` reads the interface's Roc declaration and
the generated glue and emits the Rust impl skeleton: correct mangled symbol,
correct generated argument type (`SubprocessHostExecOutputArgs` and friends are
undiscoverable otherwise), correct `decref` per the owned-argument rule
(D-U1-7). Writes `src/lib.rs` when absent, prints to stdout otherwise.

Document the sequence, because it is not obvious: for a brand-new interface the
glue does not exist until `trantor build` has run once, which composes and
glues (step 2) and then fails at the roc link on the undefined symbol. That
run is a prerequisite, and the error it ends on is expected.

Gate is exit criterion 5 — the alloc gauge, not a grep and not a bare compile.

### P2 — cargo works in a scaffolded project (D-U1-11)

Today, on a clean checkout: `cargo add image --manifest-path
components/marker/Cargo.toml` writes `image = "0.25.10"` and *then* fails,
because `trantor-abi = { path = "../../abi" }` resolves only inside the
composed copy. `cargo check`, `cargo clippy` and `cargo metadata` fail
identically, so rust-analyzer cannot load the crate.

`trantor new` emits a workspace root with `[patch.crates-io] trantor-abi =
{ path = "target/trantor/<world>/abi" }` — the shape `tests/golden/cargo-root`
already proves for this repo's own tooling. Decide and write down what happens
before the first compose, when that path does not yet exist.

Gate is exit criterion 2, on a project fresh from `trantor new`, exit code
checked. A pre-composed fixture would pass while the user's case fails.

### P3 — manifest schema (`package.toml`, `[deps]`, confinement)

- Parse `package.toml`; add `World.deps` with `{ github = … }` and
  `{ path = … }` (D-U1-12 — path deps are what make P4 and P5 testable with no
  network and no published repo).
- `#[serde(default)]` on `components` and `wiring`; `driver` becomes
  `Option<String>` resolved after dep expansion. `load_driver`,
  `default_driver_lists`, `resolve.rs:154-164` and `codegen.rs:92` all index it
  unconditionally today.
- `#[serde(deny_unknown_fields)]` throughout. Without it `driverr = "drv"` is
  discarded silently and reported as *missing* `driver` at line 1, which is
  exactly the error shape P6 exists to eliminate — and `[deps]` becoming the
  primary authored surface makes this the dominant first-user error.
- Confinement (D-U1-13): reject any `path`, `cargo_root` or `interfaces_dir`
  from a *dependency* manifest that is absolute or escapes the package root
  after normalization. Gate is exit criterion 7.
- Reserve a `version` field, parsed and ignored, so a constraint has somewhere
  to live later.
- Delete `InterfaceRef.source` — dead since it was written, and a live `[deps]`
  makes it misleading rather than merely unused.

### P4 — resolution, the lock, and `add`/`update`/`remove`

- Expansion: deps expand into `[interfaces]`/`[components]`/`[wiring]` first,
  the world's own entries override by name (D-U1-1). Unit-test that a world
  naming `fs = "fs-confined"` beats a dep defaulting to `fs-unconfined`.
- Package-qualified internal keys (D-U1-14). Dep-vs-dep on one wiring key is an
  error naming both packages. Duplicate `platform/*.roc` writes are an error, a
  `BTreeSet` in `codegen::emit`. `sanitize` collisions are an error in
  `resolve`. Gate is exit criterion 6, four fixtures.
- Overriding the inherited driver must also drop it from the expanded map
  (D-U1-6 as written did not say so): `resolve.rs:173-180` requires exactly one
  `provides_runtime`, so an override currently yields two and fails H0c.
- `git ls-remote --tags`, semver-sorted; **prefer the peeled `^{}` line** or
  the lock records a tag object rather than a commit. No tags → default branch
  HEAD via `git ls-remote --symref <url> HEAD`. Nothing touches api.github.com;
  a test asserts no code path builds that host.
- Fetch: `git clone --depth 1 --branch <ref>`, then `git rev-parse HEAD` and
  compare against the lock. Tags are mutable; a mismatch is an error. For a
  cold cache with a committed lock — the CI case — `git init && git fetch
  origin <sha> --depth 1`, since `clone --branch` cannot take a sha.
- `trantor update [<name>]` moves pins; `trantor remove <name>` removes one.
  Without these the lock's own header tells the user to edit a file they cannot
  usefully edit.
- Cache at `~/.trantor/cache/git/<org>/<repo>/<sha>/`, keyed by package
  identity. Composition stays in the project's own `target/trantor/` — a
  cross-project compose cache buys a first-time user nothing and brings shared
  mutable state that `cargo.rs:186` already needed a lock for.

### P5 — `new`, `run`, `check`, `test`

- `trantor new <name> --cli`: `world.toml`, `app/main.roc`, a workspace root
  (P2), and a `.gitignore`. The `.gitignore` is not optional — without it
  `git add -A` stages the composed tree, which is 1.8 GB for a basic-cli world.
  The old metric forbade it; D-U1-15 replaced the metric.
- Prune the 19 fixtures' stale per-module `.gitignore` lines (D-H7-38 moved
  that output under `target/`). Not every line is stale — `hc0-features`
  carries `Cargo.lock`, `b8-basic-cli` carries `examples-run/` and `dist/`.
  Exact line anchors; the real check is criterion 9's clean `git status`.
- `trantor check` — compose + `roc check`, no cargo, no link. This is the inner
  loop and the tool has never had it.
- `trantor test` — compose, `roc test` on the app, `cargo test` per host
  component.
- `trantor run [-- args]` — build then exec, with exit-code passthrough. Define
  stdin and signal behaviour.
- Project-root discovery so the commands work from a subdirectory, as cargo
  does. Every subcommand takes a world-dir positionally today.
- `trantor new-interface <name>`: scaffold `interfaces/<name>/` and
  `components/<name>-host/` plus the wiring, so P1's stub has a home.

### P6 — errors, help, and toolchain discovery

The UX is the error messages, so they are a phase and not a cleanup.

- `trantor --help` / `-h` / `help` list every subcommand. All three print
  `trantor: missing <world-dir>` today.
- Find roc on PATH before `$HOME/.bin/roc` (`build.rs:46-52`), and say what is
  missing and where it looked when it is not there. Same for `RustGlue.roc`,
  including that roc's own `roc install` can fetch a glue spec.
- A dep whose `package.toml` is missing: name the file and the repo, not a TOML
  parse error at line 1.
- A release-tag URL that 404s is indistinguishable from a repo with no release
  without the API; say the weaker true thing rather than guessing.
- Offline with a cache miss — the most likely error a user on a plane sees, and
  absent from the previous revision's list.

### P7 — the walkthrough, and an aggregate runner

`just verify` running all 20 fixtures, then `git status --porcelain` empty
(criterion 9). There is no runner today: 19 `verify.sh` files, zero references
in the justfile, so "golden suite green" has meant a human running nineteen
scripts and remembering.

Then the scenario, as `tests/golden/u1-front-door/verify.sh`, against a
`{ path = … }` baseline fixture rather than a GitHub repo:

```
trantor new resize --cli
trantor run -- big.png small.png          # imagemagick, via the baseline's subprocess
trantor new-interface resize
trantor interface-stub resize
cargo add image --manifest-path components/resize-host/Cargo.toml
trantor run -- big.png small.png          # same app code, work now in Rust
```

The app's Roc source must be **byte-identical across both runs**. If moving the
work from imagemagick into Rust changes the app, the interface boundary did not
hold and the walkthrough proved nothing.

## Risks

- **P2's `[patch.crates-io]` points into `target/`,** which does not exist
  before the first compose. If that turns out not to work from a clean
  checkout, rust-analyzer stays broken and the Rust half of the pitch goes with
  it. Measure it first inside P2; it is the phase's real content.
- **P3's schema surgery re-arms four empty-set checks.** Making `components`
  and `wiring` optional means `publish.rs:146`, `scan.rs:178`, `build.rs:207`
  and `scan.rs:141` all start seeing empty inputs as clean. The last is the
  sharpest: dep components live in the cache, so if expansion does not rewrite
  `path` to an absolute cache path, `read_dir` fails, `rust_files` returns
  empty, and every dependency's `#[global_allocator]` sails through the D-H7-13
  guard. Apply the H7 rule to all four in the same commit.
- **The `.gitignore` prune touches 19 fixtures.** Three span-based Justfile
  edits silently deleted content during H7. The mitigation is criterion 9's
  clean `git status`, which checks the effect rather than the edit.

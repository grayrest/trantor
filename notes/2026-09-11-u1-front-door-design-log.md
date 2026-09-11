# U1 — the front door: design log (2026-09-11)

**Plan:** [`plans/2026-09-11-u1-front-door.md`](../plans/2026-09-11-u1-front-door.md).

Everything below was settled against trantor `2c54e55` and roc-solid `df69d91d`.
The brief: a user starting a CLI project and adding a Rust dependency should
have roughly cargo's experience. The projects driving trantor so far have been
half a dozen worlds in one repo; the users this is for will take a baseline and
add one or two things.

The scenario used throughout, because it crosses the interesting boundary: an
image resizer that shells out to imagemagick first, then moves the work into
Rust libraries. The first half is pure Roc over a baseline. The second half is
the Tier 1 → Tier 2 cliff. The tool already understands that cliff and cannot
currently walk either side of it.

## What the tree said before any question was asked

- **The smallest working project is 10 authored files, 131 lines.**
  `tests/golden/hc0-features` prints `ping: hc0` and costs a `world.toml` (30),
  an `app/main.roc` (9), a hand-written CLI driver (18), a host component
  (33), two interfaces (24), and a `.gitignore` that lists generated paths by
  name (17). `cargo new` is 2 files and 7 lines. Five of the ten are things
  cargo never asks anyone to write.
- **There is no dependency mechanism at all.** `[interfaces.x] source =
  "roc:io/error@0.1.0"` reads like one, but `InterfaceRef.source`
  (`manifest.rs:76`) appears only in its own declaration — never read, and
  carrying `#[allow(dead_code)]` to say so. Every interface must already exist
  as a directory inside the consuming repo. No fetch, no cache, no lock, no
  version resolution.
- **Nothing consumes a published baseline.** `publish` writes `dist/` with
  platform sources, prebuilt archives and `baseline.lock`; `tier` correctly
  classifies a pure-Roc extension as Tier 1. Two fixtures assert `dist/` has
  the right files in it. No code path anywhere reads it back as a build input.
  The "no Rust toolchain" property is classified and unwalkable.
- **The ABI fingerprint is machine-local.** `publish.rs:52-62` hashes the roc
  binary's *path string*, file size, and **mtime**. Two machines with a
  byte-identical roc produce different fingerprints, so a published baseline
  can never be matched to a consumer's toolchain. Discovered while designing
  around it, not by a test — nothing compares two fingerprints today.
- **The subcommands are `compose`, `build`, `publish`, `tier`, `scan`.** All
  five operate on a world that already exists. Nothing starts one, runs one, or
  adds to one.
- **A host component's `Cargo.toml` is authored, not generated.** hc0's
  `.gitignore` tracks `components/marker/Cargo.toml` and ignores only the
  driver crate's — the composer rewrites the `[features] default` line in
  place. So `cargo add` already works on a host component; that half needs
  nothing.
- **A hosted leaf's Rust signature is undiscoverable before a build.** The
  argument type is a glue-generated name — `SubprocessHostExecOutputArgs` —
  learnable only by building and reading `abi/src/generated.rs`. Every new
  interface begins with a guess-compile-read-fix loop.
- **40 world manifests across the two repos, and some are variants of one
  composition:** `clay/world-vello.toml`, `clay/world-eink-sim.toml`,
  `b3-fs/world-confined.toml`, `b6-net/world-notls.toml`. The `--world <file>`
  flag exists for exactly this.
- **An app names its platform inside the build directory:**
  `app [main!] { pf: platform "../target/trantor/<world>/platform/main.roc" }`.
  A fresh clone cannot `roc check` until something has composed.

## Decisions

**D-U1-1 — The unit of addition is a package that declares itself.** One line
in the consumer (`[deps] resize = { github = "someone/roc-resize" }`); the
dependency's own manifest names the interface it provides, the component
implementing it, and how it wires by default. trantor expands that into the
three stanzas at compose time.

The rejected alternative was `trantor add` writing `[interfaces.x]`,
`[components.x-host]` and the `[wiring]` line into the consumer's world.toml.
It keeps every wiring decision visible, which is genuinely worth something —
but a dependency can then never carry its own requirements, and the moment
`resize` needs `io` the user is hand-resolving a dependency graph in TOML.

The explicit three sections stay, and stay authoritative, as the override. That
is not a concession: it is what lets roc-solid swap `fs-confined` for
`fs-unconfined` and `rusqlite` for `turso`, which is the whole point of the
wiring table. Nobody resizing a PNG should have to know it exists.

**D-U1-2 — `package.toml` is a second file, not a renamed `world.toml`.** The
variant mechanism decides this. A *composition* legitimately has variants —
same components, different wiring, which is what `world-vello.toml` and
`world-confined.toml` are. An *offer* does not: `roc-resize` provides one
interface no matter who consumes it. Two shapes, two files, and zero migration
for the 40 manifests that exist.

A publishing repo carries both: `package.toml` for consumers, `world.toml` to
compose and test its own component against a driver locally. That is cargo's
split between the `[package]` stanza and dev-dependencies, in two files rather
than sections. The merged `trantor.toml` was considered and dropped — it reads
as more cargo-like and is strictly worse here, because `trantor-vello.toml` is
not a sentence anyone can parse.

**D-U1-3 — GitHub, resolved with git and plain HTTPS, never the REST API.**
`trantor add <org>/<repo>` resolves to the newest semver tag, or to the default
branch's HEAD when the repo has no tags, and pins the commit sha in
`trantor.lock`.

Mechanism matters more than it looks: `git ls-remote --tags` lists tags,
`git clone --depth 1 --branch <tag>` fetches, and a release asset lives at the
predictable `https://github.com/<org>/<repo>/releases/download/<tag>/<name>`.
None of that touches api.github.com, so `trantor add` needs no token, hits no
unauthenticated rate limit, and works behind a proxy that allows git and HTTPS.
Reaching for the REST API to list tags would have bought nothing and cost the
no-auth property.

**D-U1-4 — Prebuilt baselines ride GitHub release assets.** *(DEFERRED by
D-U1-9; kept here because the design is settled and only the timing moved.)* A release may carry
`baseline-<fingerprint>.tar.zst`. If the consumer's fingerprint matches one,
trantor unpacks it and skips cargo and roc glue entirely — Tier 1, no Rust
toolchain. If nothing matches, it clones the source and composes into a
content-addressed cache at `~/.trantor/cache/<fingerprint>/`, paid once per
machine instead of once per project.

The cost lands on the publisher: every roc version bump means cutting a release
with fresh assets, or consumers silently fall back to needing cargo. Accepted,
because the alternative — always build from source — makes the tier classifier
describe a distinction nobody can act on, and makes a first-time user install a
Rust toolchain to print a string.

**D-U1-5 — The ABI fingerprint becomes portable, and this is a precondition,
not a detail.** *(DEFERRED with D-U1-4, less the fail-open fix, which lands
now — see D-U1-10.)* Hash the roc version string and the glue spec's content; never
a path, a file size or an mtime. D-U1-4 is meaningless without it, and the
current fingerprint's failure is invisible — it produces a plausible hex string
that simply never matches anyone else's.

**D-U1-6 — A project inherits its driver from its baseline.** A world with
exactly one baseline dependency takes that baseline's driver and authors none.
`driver =` stays, and is what you write when your app is a genuinely different
shape. The minimum CLI project is then a four-line `world.toml` and an
`app/main.roc`, with no driver, no interfaces directory, no components
directory, and no Cargo workspace.

Rejected: trantor shipping built-in `cli` and `reactor` drivers. It gives the
same minimal manifest and makes the driver trantor's private surface rather
than an ordinary component, which cuts against everything else being
composable. Also rejected: scaffolding a driver copy into every project, which
means a driver fix requires every project to regenerate.

**D-U1-7 — `trantor interface-stub <interface>` emits the Rust signature.**
Read the Roc declaration and the generated glue, print the exact
`#[unsafe(no_mangle)] extern "C-unwind" fn trantor__<c>__<stem>` with the real
argument type and the `decref` the owned-argument rule (B0) requires. This is
the closest analogue to what cargo actually provides: the compiler telling you
the signature you were supposed to write.

Independent of D-U1-1 through D-U1-6 and worth doing regardless of how any of
them had gone.

**D-U1-8 — cargo stays cargo.** Adding a Rust crate to a host component is
`cargo add --manifest-path components/<c>/Cargo.toml`, unwrapped, rather than
behind a trantor surface.

*(The second half of this decision was **WRONG** and is corrected by D-U1-11:
it claimed cargo works on an authored component today. It does not.)*

What it does need is the failure message. Two components vendoring one native
is caught by the H0c scan, which is a real advantage over `cargo add` — cargo
will happily link zlib twice. It is also a failure mode a first-time user will
hit with no idea what it means, so the scan's message must name both
components, name the symbol, and point at `shared_symbols`.

## Independent review (2026-09-11) — what it measured

Three reviewers read the plan against the tree rather than against its own
prose. Everything below was re-verified here before being recorded.

- **A published baseline cannot link.** `publish` skips `test_only` archives
  (`publish.rs:88-99`) but the published `main.roc` still lists
  `libtestnet_host.a` in `inputs` for both targets and still binds
  `trantor__testnet_host__start_test_server`. dist ships 12 archives and that
  is not one of them. `b8-basic-cli/verify.sh:155` asserts the archive is
  absent and calls that a pass.
- **The rest of a relocated `dist/` does work.** A reviewer ran it: PATH
  scrubbed, workspace and abi crate deleted, one archive hand-copied — `roc
  check` passes, `roc build` links, the binary runs. The emitted `main.roc` has
  no absolute path assumptions (`inputs_dir: "targets/"` is platform-relative).
  So the Tier-1 premise held; it was never the risk.
- **`abi_fingerprint` fails open at every step.** `hash_file` returns the
  accumulator unchanged on a read error and the roc block sits inside `if let
  Ok(md)`. Measured: with `ROC` and `GLUE` pointing at nothing, `publish`
  emits `abi_fingerprint cbf29ce484222325` — the bare FNV offset basis,
  identical on every machine. The "agrees with everything" failure the plan was
  written to prevent is already in the shipping code.
- **The fingerprint has no architecture in it.** `codegen.rs:296-297` hardcodes
  `arm64mac`/`x64mac`; cargo builds for the host and stages into `arm64mac`
  whatever the host is. An x64 publisher's asset matches an arm64 consumer's
  fingerprint exactly.
- **`trantor tier` is a fifth instance of the silent-check pattern.** `tier
  tests/golden/b8-basic-cli/extension` prints Tier 1; `compose` on the same
  directory refuses it (`[world].driver 'main-driver' is not a declared
  component`). `classify` (`publish.rs:139-150`) only asks whether any
  component is `kind = "host"`, and that manifest declares none at all. Green
  on an unbuildable world since it was written.
- **`cargo add` fails on a scaffolded project.** Measured on a clean checkout
  of `hc0-features`: `failed to read .../abi/Cargo.toml: No such file or
  directory`, and it writes `image = "0.25.10"` into the manifest *before*
  failing. `cargo check`, `cargo clippy` and `cargo metadata` fail the same
  way, so rust-analyzer cannot load the crate at all. The monorepo does not hit
  this because `tests/golden/cargo-root/Cargo.toml` carries a
  `[patch.crates-io]` at the workspace root; a scaffolded project has none.
- **The plan's own four-line `world.toml` does not parse.** `driver`,
  `components` and `wiring` are all non-defaulted (`manifest.rs:15-26`).
- **Exit criterion 7 was already implemented.** `scan.rs:225-236` names both
  components, the symbol and `shared_symbols`; `nm-scan/verify.sh:26-27`
  already asserts all three.
- **`trantor --help` prints `trantor: missing <world-dir>`.** roc is resolved
  as `$ROC` or `$HOME/.bin/roc` with no PATH fallback (`build.rs:46-52`), so
  the front door fails on any machine but this one.
- **The 19 fixture `verify.sh` scripts have no aggregate runner** (0 references
  in the justfile). "Golden suite green" is a human running nineteen scripts.
- **A `dist/` is 214 MB.**

## Decisions from the review

**D-U1-9 — Prebuilt baselines wait for a second publisher.** Track B — the
portable fingerprint, release assets, and Tier-1 consumption — is deferred, not
cancelled. The trigger is someone other than this repo's author wanting to
publish a baseline.

The reason is arithmetic, not doubt about the design. The fingerprint must
cover the glue spec, and the glue spec is `RustGlue-b07d7e-rebased-main.roc`,
personally maintained alongside a personally-rebased roc. A third party cannot
produce a matching asset without byte-identical copies of both, so today the
set of people who can publish a consumable baseline is one, and the set who
benefit is zero. Add 214 MB per baseline per roc version per architecture as
release assets. Deferring costs the "no Rust toolchain" claim and nothing else.

What survives intact is the part that mattered: a dependency still comes from
GitHub with one line, and D-U1-6 still holds — a project still authors no
driver, it just composes the baseline's driver from source the first time.
Needing cargo is what every Rust project needs, and it is honest.

This also closes an open item by removing it. The platform is now always
composed locally into `target/`, so the app's `platform "…"` path is exactly
what it is today and needs no cache-relative or absolute-path scheme.

**D-U1-10 — Deferred code must not lie while it waits.** `abi_fingerprint`
becomes a hard error when roc or the glue spec is unobtainable, rather than
silently omitting the term; `classify` resolves the world and fails loudly
rather than reading an empty component map as purity. Both are small, and the
alternative is leaving two commands that emit confident wrong answers for
however long Track B waits. The `test_only`/`dist` inconsistency is recorded
above rather than fixed, because fixing it needs a Track B decision (strip the
component from `archive_order` and the published `main.roc` diverges from the
one that was tested; keep it and the baseline ships test scaffolding).

**D-U1-11 — Making cargo work on a scaffolded component is trantor's job, and
it is Track A's headline.** D-U1-8's second half was wrong. With Track B gone,
`cargo add` is the *entire* Rust-dependency story, and it currently fails and
half-applies. `trantor new` emits a workspace root carrying `[patch.crates-io]
trantor-abi = { path = "target/trantor/<world>/abi" }` — the shape
`tests/golden/cargo-root` already proves for this repo's own clippy and tests.
That also restores rust-analyzer, which is the larger daily cost.

**D-U1-12 — `{ path = … }` deps are first-class, not a convenience.** Every
phase of Track A would otherwise be untestable until a baseline repo exists on
GitHub, which is a phase-ordering defect: the fixtures need a dependency they
can point at with no network. Path deps make the whole system exercisable in
`tests/golden/`, and they are what a user editing two related packages wants
anyway.

**D-U1-13 — A dependency's manifest is confined.** `component_dir`,
`cargo_root` and `interfaces_dir` are `world_dir.join(…)` with no
normalization (`manifest.rs:71-73,155-173`). `cargo_root` becomes cargo's
working directory, so a dep declaring `cargo_root = "../../.."` runs
`cargo build` in a directory of its choosing against whatever `Cargo.toml` and
`build.rs` live there. Reject any path from a *dependency* manifest that is
absolute or escapes the package root after normalization.

Compiling a dependency's `build.rs` at all is cargo's trust model and is
accepted, deliberately and out loud. Letting a manifest choose the directory
cargo runs in is not, and closing it is ten lines.

**D-U1-14 — Names are package-qualified internally.** Five surfaces collide
across two dependencies with no diagnostic today: Roc module filenames written
flat into `platform/` (b8 already needed `exports = ["Path as StrPath"]` to
disambiguate two path packages *within one repo*), archive filenames after
`sanitize` maps every non-alphanumeric to `_` (`my-fs` and `my_fs` → one
`libmy_fs.a`), the `[packages]` alias map, dep-vs-dep wiring of the same key,
and a local component name shadowing a dep's. Interfaces and components carry
their package as an internal key; the world's own entries keep the bare
spelling and win. Dep-vs-dep on one wiring key is an error naming both
packages, not a silent last-wins.

**D-U1-15 — The gate measures errors, not file counts.** "2 authored files,
≤15 lines" is gameable by relabelling files generated and is already distorting
the design: it forbids the `.gitignore` that keeps a 1.8 GB composed tree out
of `git add -A`, and forbids the workspace root that D-U1-11 needs. Replaced
by: every command in the walkthrough exits zero and prints no error text, and
the hand-written `world.toml` diff at each step is recorded. Those measure the
experience; a file count measures the template.

## Still open (raised, not decided)

- **`out_dir` keys on the world DIRECTORY name while the app hardcodes that
  name** in its `platform "…"` path. `git clone resize my-resize` composes to
  `target/trantor/my-resize/` while the app imports `target/trantor/resize/`,
  so nothing composes to the path the app names. Predates this pass and is not
  caused by it, but `trantor new` makes it every user's clone.
- **A fresh checkout cannot `roc check` before composing,** which also blocks
  `roc test` and the LSP. Defensible by analogy to `cargo build` and
  `target/debug`; a measurement of what roc's tooling actually does with a
  missing platform path should settle it before anyone designs around it.
- **Version constraints have no spelling.** `[deps]` is `{ github = … }` /
  `{ path = … }` with no `^1.2`, so the deferred transitive-conflict problem is
  currently not merely unresolved but unexpressible. Deferring the resolver is
  fine; shipping a syntax with no room for a constraint is not, and P3 should
  reserve the field even while ignoring it.
- **macOS only.** `nm -m`, Mach-O as the only native reader, `xcrun`,
  `std::os::unix::fs::symlink`, and `arm64mac`/`x64mac` hardcoded in
  `codegen.rs:296-297`. A legitimate scope choice that "the front door" implies
  otherwise; the plan states the matrix in one line rather than pretending.
- **Deferred with Track B (D-U1-9):** the `test_only`/`dist` inconsistency, the
  empty `macos-sysroot` in a published baseline, architecture in the asset
  name, whether appending a hosted leaf relayouts shared glue types (the
  mixed-mode question that decides where the Tier-1/Tier-2 boundary really
  falls), asset integrity and TOFU, and cache eviction.

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

**D-U1-4 — Prebuilt baselines ride GitHub release assets.** A release may carry
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
not a detail.** Hash the roc version string and the glue spec's content; never
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
`cargo add --manifest-path components/<c>/Cargo.toml`, unwrapped. The component
manifest is already authored (see above), so this works today and needs no
trantor surface.

What it does need is the failure message. Two components vendoring one native
is caught by the H0c scan, which is a real advantage over `cargo add` — cargo
will happily link zlib twice. It is also a failure mode a first-time user will
hit with no idea what it means, so the scan's message must name both
components, name the symbol, and point at `shared_symbols`.

## Still open (raised, not decided)

- **The app's platform path points into `target/`.** A fresh clone cannot `roc
  check` before composing. Defensible by analogy — `cargo build` is likewise a
  prerequisite for `target/debug` — but an editor or LSP sees a broken import
  on a clean checkout, which cargo users do not experience. Not decided; it
  wants a measurement of what roc's tooling actually does with a missing
  platform path before anyone designs around it.
- **Version conflicts between transitive deps.** Two packages wanting different
  versions of one interface. Nothing in this pass resolves it; the first
  package graph deep enough to hit it decides the policy.
- **Bootstrapping.** None of this works until a baseline exists as its own
  GitHub repo with tags and release assets. That is a work item in the plan
  (P6), not a decision, but it is worth saying plainly that the front door
  opens onto nothing until it is done.

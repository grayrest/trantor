# hematite — design log (D1–D23)

**Plan:** [`plans/2026-09-04-hematite-v1.md`](../plans/2026-09-04-hematite-v1.md).
**Rendered:** https://claude.ai/code/artifact/3cf075ab-dfb4-4ed8-a7cb-e6ad542e94fd

Decisions are numbered in resolution order; each depends on those above it. The
context is that upstream Roc has declined these design goals and directed that
they be solved at the platform level.

## Evidence the decisions rest on

Measured 2026-09-03/04 across `seahaven`, `roc-solid`, `roc-solid/platform-im`
and `tower-platform`.

| platform | hosted symbols | `requires` shape | effect model |
| --- | --- | --- | --- |
| seahaven | ~60 | `main!` only | app calls host |
| roc-solid | 18 | tree + boxed closures | both directions |
| roc-solid im | **0** | `names, init, view…` | host calls app; effects as `Cmd` data |
| tower | — | `settings, routes, init!` | host owns async server loop |

Four measurements did most of the work:

1. **The derived layer dominates.** `Path.roc` is 765 lines over ~12 host
   primitives; `Stdout.roc` is 36 lines of passthrough over 3. ~1,500 of
   seahaven's 2,325 Roc lines are implementation-independent — and would drift
   if each implementation shipped a copy.
2. **`IOErr` is nominal** (`IOErr := [...]`) and imported by 9 of 15 modules.
   Structural duplication across interfaces is therefore impossible: two
   definitions are two distinct types, and `roc glue` would emit two unrelated
   Rust enums.
3. **Row polymorphism never crosses the ABI.** The host boundary uses *closed*
   unions (`Try({}, [StdoutErr(IOErr)])`); the derived layer widens to open ones
   (`[StdoutErr(IOErr), ..]`). The WIT-expressible fraction is much larger than
   it first appears.
4. **Roc values are `!Send`.** `executor.rs`: "no Roc-refcounted value ever
   crosses to a worker thread." All Roc calls are on the runtime thread; async
   results arrive via a `Sink` that is already abstract over winit and mpsc.

## Decisions

**D1 — Build-time composer emitting a stock Roc platform.** Not a compiler
feature, not runtime dispatch. `inputs:` is a list *syntactically*, but **no
platform in the tree has ever put more than one host archive in it** — every
`targets:` block is `["libhost.a", app]`. Multi-archive linking is therefore the
central UNVERIFIED assumption, not a supported shape, and it is why H0a runs
first and is a hard NO-GO. Runtime composition parked for wasm only (see D22).

**D2 — Interfaces are bodiless; the derived layer is a pure-Roc component.**
An interface declares types and signatures only, as in WIT. `Path.roc`'s 765
lines become a component importing the primitive fs interface and exporting the
rich one, with no host archive. Buys: pure-Roc components as first-class units;
seahaven's confinement as composition rather than a fork; and somewhere for
interposition to live.

**D3 — WIT-shaped superset, projectable subset, reported not enforced.** Core
type system and syntax from WIT; named Roc extensions for `box`, closures and
refcount ownership. Strict WIT would force a redesign of roc-solid's
closure-passing boundary — the thing being migrated as a test of scaling. Even
seahaven doesn't fit: `utc_now!` returns `U128` and WIT tops out at `u64`.

**D3a (from roc-solid G1, adopted as a notation rule) — closures in the IDL
carry closed concrete dep types.** G1 found that funnel-crossing closures with
inferred open-row or unbound dep types typecheck, build, and then *silently
corrupt memory when fired*, under mismatched monomorphization layouts through
the erased downcast. Under-annotated closures must be inexpressible in the
notation, not merely discouraged.

**D4 — Source composition, not prebuilt archives.** Generate `main.roc` → one
`roc glue` run → cargo workspace → one `libhost.a`. `roc glue` is
whole-platform by construction and the design log (roc-solid D10) requires
exactly one ABI crate. Prebuilt archives would need unproven per-interface glue
stability and would turn a stale-glue mismatch into a silent link success.

**D5 — Exactly one driver per world.** One component owns `requires`,
`provides`, and native `main()`; everything else is imports-only. This is
WASI's command/reactor split. A generated composite `requires` would widen a
surface whose own comment warns a mis-shaped field is "a runtime platform
requirement failed checking, or a SIGSEGV." **Amended by D18-C:** a driver whose
`requires` names types owned by other components (roc-solid `platform-im`
references `Element`, `Env`, `Cmd`, `Scene`, `CapVal`, `Id`) must declare those
dependencies in a `[requires.uses]` manifest block. "One driver, opaque
contract" is really "one driver plus its declared type imports."

**D6 — DAG wiring internally, flat app-visible surface.** The internal graph may
contain an interface many times (`fs = audit-log(cap-std-fs)`); exactly one
implementation reaches the app. Internal symbols mangled **readably**:
`hematite__seahaven_fs__file_read_bytes`. Numeric mangling was considered and
rejected — link errors are where the reader has least context, and H0c is
specifically a duplicate-symbol spike.

**D7 — Mechanism, not vocabulary.** No blessed core types; `roc:cli` is a
userland package owning `IOErr` and `OsStr`. The vocabularies genuinely don't
intersect — roc-solid's async errors are bare `Str`, tower's are
`[UniqueViolation, ForeignKeyViolation, Busy, …]`. `wasi:io` is not part of WIT
either. Promote only what two independent worlds both need.

**D8 — A minimal C-ABI `HostCtx`.** Thread-affinity guarantee, wake/completion
sink, `init`/`shutdown` lifecycle. Free-standing — *not* riding on `RocHost`,
which the upstream glue redesign deletes (Phase 2, items 19–21). C-ABI rather
than a Rust trait so Zig stays viable. roc-solid's `Sink` is already this
abstraction at N=1.

**D9 — Opaque completions, per-component supersession.**
`wake(component_id, *mut c_void)`; the component allocates and frees, the driver
couriers. Erasure buys language-neutrality free — no canonical-ABI lowering
needed for a Zig component — and confines `unsafe` to two generated functions.
`DispatchId { slot, generation }` stays inside the component.

**D10 — Both platform-author and app-author composition.** A baseline world is
published and consumed by URL; an app needing more declares its own world. The
disease being cured is visible in `crates/host-im/`: `dbx.rs`, `docsvc.rs`,
`audio.rs`, `eink/` — app-specific host code inside the shared platform host,
because there was nowhere else to put it.

**D11 — Two extension tiers, cliff visible.** Tier 1 adds pure-Roc components
using interfaces the baseline already provides: no new hosted symbols, so glue
output is unchanged and the prebuilt `libhost.a` stays valid — **no Rust
toolchain**. Tier 2 brings host code and triggers full source composition.
Justified by a hard fact: Roc packages are `package [...] {}` with no `hosted`
and no `requires`, so a package structurally *cannot* source effects. That is
why `Path.roc` lives in `platform/`, and it is the gap only hematite can fill.

**D12 — Roc code may shim interfaces; state via a `cell` component.** Typed and
handle-based (`Cell.new!`/`get!`/`set!`), living in the baseline as userland
code. roc-solid's `stash_put!` slot design was forced by its dependency graph;
a general cell has no excuse for "slot ids must uniquely identify the payload
type" being a comment rather than a check.

**D13 — Symbolic imports, generated binding modules.** Source says `import fs`;
hematite emits one internal binding module per wiring point — bodiless
declarations bound in `hosted {}` for a host implementation, forwarding
functions for a Roc shim. **Component source is never rewritten.** Needs no Roc
AST rewriter, and makes implementation language invisible at the call site by
construction. Generalises `Host.roc` + `hosted {}` from one to N. **Scope of
"never rewritten":** this governs *composition* of already-decomposed
components. Decomposing a legacy platform (seahaven's `import Host` →
per-interface `import fs`) is a one-time authored migration, not composition —
migration ≠ composition. See R8.

**D14 — Explicit world export list; worlds may rename.** Apps import
package-qualified (`import pf.Stdout`) but everything lands flat in `pf.*`.
Collisions are a hard compose error; the newly added component pays the rename.
Seahaven already keeps 3 of 16 modules internal — this makes that list
first-class.

**D15 — Hematite does not generate host-side stubs.** Consume upstream glue;
`RustGlue`, `ZigGlue` and `CGlue` already exist with an ABI risk register and a
glue runtime matrix. Forking a spec would duplicate work upstream and carry a
standing ABI-churn tax. **Accepted gap:** nothing checks a hand-written
`#[no_mangle] extern "C"` against its Roc declaration.

**D16 — One interface version per world.** Semver unification picks one;
versions are recorded but Roc's typechecker arbitrates. Multi-version is
actively hostile under nominal typing — two `IOErr`s in one heap, printing
identically, with no lever for the app author.

**D17 — Spike first, hand-compose, then extract the tool.** Mirrors
G0/G1/G2. The hand-written composition becomes the golden fixture, so the test
strategy is "does the tool regenerate it byte-for-byte" rather than asserting
properties of generated Roc.

**D18 — Two manifest formats, mirroring WIT and wac.** `.wit`-shaped for
interfaces and abstract worlds; TOML for compositions. Cramming source
locations, frameworks and wiring into `.wit` with custom attributes would break
the projection it was chosen for. The driver's `requires` — 60+ lines with an
app-typed `[Model : model]` parameter — is spliced through as literal text;
hematite does not parse the Roc.

**D18-C — resolving the splice-vs-rename contradiction.** Splicing `requires`
verbatim contradicts D14 (rename on collision — `Cmd` collides concretely:
seahaven's `Cmd :: { … }` structural process command vs `platform-im`'s
`Cmd := [ … ]` nominal UI command) and D13 (binding-module indirection — this
compiler *segfaults with no diagnostic* on a type alias imported across type
modules, and on an import missing from `exposes`). Resolution: the driver
declares the cross-component types its `requires` names in a `[requires.uses]`
TOML block — a WIT-style `use` list. Hematite still splices the `requires` text
verbatim (never parses Roc), but resolves those declared identifiers to their
post-rename canonical modules, guarantees each is in `exposes`, and requires
D13 binding modules to **re-export the original nominal rather than alias it**
for any type a driver `requires` names. Rejected alternatives: (A) hematite
parses the `requires` block — reintroduces the Roc parser D13 exists to avoid;
(B) forbid renames touching a `requires` type — guts D14's "new component pays"
rule. Spiked at H1b before any of it is built, because the failure mode is a
silent segfault, not a compose error.

**D19 — Roc shims serve Roc consumers only (v1).** Host→Roc would need a third
lifecycle phase for closure registration, runtime-thread confinement, and
re-entrant Roc→host→Roc crossings. The line isn't arbitrary: async interfaces
are host-shim territory because an async consumer is a worker thread by
construction. Faking HTTP in roc-solid means swapping the host component, which
the DAG already supports.

**D20 — Quiesce is an interface, not vtable surface.** `roc:test/quiesce` with
`is-idle`, exported by components with in-flight work. roc-solid ships ~80 gates
with no quiesce primitive, but that is evidence about a single-effect-source
architecture, not about composition.

**D21 — Vtable admission rule: soundness in, services out.** Something joins the
vtable only if a component cannot uphold it alone *and* getting it wrong
corrupts memory rather than producing a wrong answer. Logging, config, tracing,
metrics and error reporting all fail this and will all feel like vtable material
during migration. The rule matters more than any single ruling.

**D22 — Wasm: target first (a), outer-edge `.wit` second (b), full component
composition a non-goal (c).** Roc is shared-everything, so a component boundary
can only exist where no Roc value crosses — the outer edge. The existing wasm
target is *browser* wasm (`dom.rs`, `recon.rs`, JS imports), where WASI is
irrelevant; the outer-edge description must be pluggable per target. (c) buys
deploy-time substitution that source composition already provides at build time,
at the cost of a copying adapter per call.

**D23 — Provenance in generated output; nothing generated in git.** File headers
carry world, hematite version and ABI fingerprint; declarations carry origin
comments; output is deterministic. Ephemeral in `target/hematite/` for apps,
published tarballs for worlds. Diagnostic rewriting rejected: roc's output
format moves, and a tool that lies about locations is worse than one that points
at generated code honestly.

## Open items

1. **Unchecked host implementations** (D15). ~60 hand-written externs in
   seahaven alone. Upstream's ABI fingerprint covers version drift, not
   per-symbol signatures. Revisit if H5 surfaces a real mismatch.
2. **No `-l` mechanism in the roc linker.** Frameworks arrive via TBD stubs in
   the sysroot; ordinary libraries have no documented path, so components must
   vendor them. H0c determines whether two vendoring components can coexist.
3. **What the baseline world contains.** Deliberately deferred — it should be
   discovered by H5 and H7, not decided in advance. That discovery is the
   experiment.
4. **Ecosystem lockstep** (D16). One component pinning an old interface blocks
   everyone. Compatible version ranges are the eventual softener; currently just
   a resolver error message.
5. **Binding-module indirection cost** (D13). Measured in H3 at `--opt=speed`.
   If the hop doesn't flatten, the fallback trades D12's implementation
   transparency for direct calls.

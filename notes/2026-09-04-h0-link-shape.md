# H0 — link-shape spike outcomes

Gate H0 of [`plans/2026-09-04-trantor-v1.md`](../plans/2026-09-04-trantor-v1.md).
Spike code under `spikes/h0/`. Compiler: `~/.bin/roc` release-fast-43746ac5,
glue `~/.bin/RustGlue.roc`.

## H0a — two-archive link ✅ PASS

Two independently built Rust staticlibs, `inputs: ["liba.a", "libb.a", app]`,
one hand-written `platform/main.roc`. `roc build` links both and the binary
runs.

- `liba.a` (comp-a): the six runtime symbols (`roc_alloc` … `roc_crashed`), the
  process `main()` that calls `roc_main()`, and hosted `trantor__a__ping`.
- `libb.a` (comp-b): hosted `trantor__b__pong` **only** — no runtime, no glue,
  no main.
- app calls both via `pf.Effect`; returns `40 + 2`. Binary prints `42`, exit 0.

This was the plan's #1 fatal risk (R1): no platform in the tree had ever linked
more than one host archive. **Cleared.** Multi-archive linking with
`inputs:` as a genuine list is real, not just syntactic.

## H0e — multi-hosted-module composition ✅ PASS

The `hosted {}` block names symbols implemented across two separate archives,
and the linker resolves the union. Verified by `nm`:

| symbol | liba.a | libb.a |
| --- | --- | --- |
| `trantor__a__ping` | **T** (defined) | — |
| `trantor__b__pong` | — | **T** (defined) |
| `roc_alloc` (runtime) | T | — |

So the "mangled union" hosted shape the architecture diagram promises is
accepted by this compiler. A driver archive can carry the runtime + entrypoint
while effect archives carry only their hosted symbols — which is exactly the
D6/D8 split (one driver owns `main()`, components are imports-only).

## Learnings folded back

1. **Readable mangling (D6) works at the linker.** `trantor__a__ping` links and
   `nm` reads cleanly. No numeric mangling needed.
2. **Runtime symbols must live in exactly one archive.** comp-b deliberately
   omits them; had both archives defined `roc_alloc`, we'd expect a duplicate
   symbol — this is the H0c question for *vendored* natives, and the same
   discipline (exactly one provider) is the answer. Trantor must assign the
   runtime + entrypoint to exactly one component (the driver) by construction.
3. **Block bodies need braces** in this compiler: `|{}| { a = …; b = …; expr }`,
   not indentation. A codegen detail for the generated `main.roc`/binding modules.
4. **`roc build --output=DIR/x` does not create DIR.** The generated build
   invocation must `mkdir -p` first (this cost one failed link that looked like
   a link error but was a missing directory).

## H0d — panic/unwind across component frames ✅ PASS, with a hard rule

`spikes/h0d/`. Component B's hosted `boom!` panics while holding a local RAII
guard; the driver (component A) wraps `roc_main()` in `catch_unwind` and holds
its own process guard. Two ABI configs:

| config | outcome | B::drop (own frame) | driver catch | A::drop | process |
| --- | --- | --- | --- | --- | --- |
| `extern "C"` (default) | **abort** — "failed to initiate panic, aborting" | ✗ | ✗ | ✗ | SIGABRT (134) |
| `extern "C-unwind"` | unwind crosses the Roc frames | ✅ | ✅ | ✅ | survives (0) |

**The rule (folded into D8):** every trantor-generated hosted-symbol boundary
**and** the driver's `roc_main` import must be `extern "C-unwind"`. Default
`extern "C"` makes any panic an unconditional abort with **zero** teardown — no
component cleanup at all.

**What the admission rule (D21) gets:** even under `C-unwind`, only teardown
that is RAII-local to the panicking component's **own hosted-call frame** runs.
Roc-frame cleanup still does not (consistent with G1's D11d — the Roc values
leak). So a component must release in its own frame or register with the driver;
it must never rely on Roc unwinding its resources. Partial teardown was observed
to be *safe* here (B's frame cleaned, driver caught, no corruption), so "catch
at the boundary and continue/report" is a viable driver policy, not forced abort.

## H0c — duplicate vendored natives ⚠ PASS-WITH-POLICY (the footgun is real)

`spikes/h0c/`. Two components each `cc`-compile a C object defining the **same**
symbol `vendored_answer` (simulating two archives each bundling sqlite). Each
component's hosted fn calls its own copy, forcing both objects to load.

First, a false alarm worth recording: an early run reported "missing host symbol
`trantor__a__ping`" and looked like a roc-linker duplicate-symbol failure. It
was not — a bad edit had orphaned the `#[unsafe(no_mangle)]`, so ping got
mangled and genuinely left the archive. **The roc error was correct.** Lesson
for codegen: a dropped `no_mangle` presents exactly as a missing-host-symbol
link error; trantor's generated host stubs must carry it unconditionally.

The real result, once the spike was correct:

- Single archive + vendored `cc` object → **links and runs fine** (`spikes/h0c2/`,
  output 40). Vendoring itself is not the problem; `cargo` bundles the `cc`
  object into the crate's staticlib (`nm` confirms `_vendored_answer` inside
  `libcomp_a.a`).
- Two archives, **same** vendored symbol → **links silently, first-in-`inputs`
  wins.** `["liba.a","libb.a",app]` → output **80** (both calls hit comp-a's
  copy, 40+40). Swap to `["libb.a","liba.a",app]` → output **4** (both hit
  comp-b's, 2+2). No error, no warning; the loser's calls are silently
  dispatched to the winner's code.

**Why it matters:** two components bundling *different versions* of the same
native lib (two sqlites with different struct layouts) would silently share one
version — memory-unsafe, and invisible at link time. The roc linker will not
catch this. Neither will `ld`.

**Policy (H0c's required deliverable):** at compose time trantor must scan each
component archive's defined global symbols (`nm`) and **reject any non-hosted,
non-runtime symbol defined by more than one component**, unless the world
explicitly declares it a shared/deduplicated native dependency. Corollary: the
"exactly one runtime provider" rule (roc_alloc … from H0a) is *not* enforced by
the linker either — two archives defining `roc_alloc` would also silently
first-wins — so trantor must own that as the same compose-time check.

## H0b — framework sysroot union ✅ PASS

`spikes/h0b/`. comp-a's `ping` references CoreFoundation
(`CFAbsoluteTimeGetCurrent`), comp-b's `pong` references Security
(`SecRandomCopyBytes`) — disjoint frameworks.

- **Control (no sysroot):** `ld64.lld: undefined symbol: _SecRandomCopyBytes`.
  (CoreFoundation resolves via base System libs; Security needs its TBD.) Proves
  the sysroot's framework set is what drives linkage.
- **Union sysroot** (`macos-sysroot/System/Library/Frameworks/` with
  `CoreFoundation.framework` **and** `Security.framework` TBD symlinks, built by
  roc-solid's `sysroot` recipe rule) → **links and runs, output 42.**

The roc linker (`cli/linker.zig findPlatformSysroot`) discovers
`platform/targets/macos-sysroot` by convention and auto-adds `-framework X` for
each `X.framework` carrying a TBD. So the framework directory **is** the
dependency declaration, and trantor's job is purely to emit the *union* of
every component's declared frameworks into that one directory. Confirmed the
union is exactly the two needed and nothing leaks. Mechanism is a directory of
symlinks — no linker flags, no per-component negotiation.

## H0 gate: ✅ COMPLETE (a, b, c, d, e)

| spike | verdict |
| --- | --- |
| H0a two-archive link | ✅ PASS |
| H0b framework sysroot union | ✅ PASS |
| H0c duplicate vendored natives | ⚠ PASS-with-policy (silent first-wins; trantor must nm-scan) |
| H0d panic/unwind across frames | ✅ PASS (requires `extern "C-unwind"`) |
| H0e multi-hosted-module union | ✅ PASS |

No NO-GO. D1's central assumption (multi-archive linking) holds. The two things
trantor must own that the linker will not enforce: **exactly one runtime
provider** and **no duplicate non-hosted symbols** (both from H0c's mechanism),
and every generated boundary must be **`extern "C-unwind"`** (H0d).

## Recurring codegen hazard (met twice)

A dropped `#[unsafe(no_mangle)]` makes a hosted symbol vanish from the archive
and presents *identically* to a roc "missing host symbol" link error — not as a
Rust error. Bit the spike twice via edits that orphaned the attribute onto an
adjacent `extern` block. Trantor's generated host stubs must emit `no_mangle`
unconditionally and ideally assert each expected symbol is present in the built
archive (an `nm` post-check) before invoking `roc build`.

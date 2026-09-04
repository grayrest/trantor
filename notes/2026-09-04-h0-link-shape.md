# H0 — link-shape spike outcomes

Gate H0 of [`plans/2026-09-04-hematite-v1.md`](../plans/2026-09-04-hematite-v1.md).
Spike code under `spikes/h0/`. Compiler: `~/.bin/roc` release-fast-43746ac5,
glue `~/.bin/RustGlue.roc`.

## H0a — two-archive link ✅ PASS

Two independently built Rust staticlibs, `inputs: ["liba.a", "libb.a", app]`,
one hand-written `platform/main.roc`. `roc build` links both and the binary
runs.

- `liba.a` (comp-a): the six runtime symbols (`roc_alloc` … `roc_crashed`), the
  process `main()` that calls `roc_main()`, and hosted `hematite__a__ping`.
- `libb.a` (comp-b): hosted `hematite__b__pong` **only** — no runtime, no glue,
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
| `hematite__a__ping` | **T** (defined) | — |
| `hematite__b__pong` | — | **T** (defined) |
| `roc_alloc` (runtime) | T | — |

So the "mangled union" hosted shape the architecture diagram promises is
accepted by this compiler. A driver archive can carry the runtime + entrypoint
while effect archives carry only their hosted symbols — which is exactly the
D6/D8 split (one driver owns `main()`, components are imports-only).

## Learnings folded back

1. **Readable mangling (D6) works at the linker.** `hematite__a__ping` links and
   `nm` reads cleanly. No numeric mangling needed.
2. **Runtime symbols must live in exactly one archive.** comp-b deliberately
   omits them; had both archives defined `roc_alloc`, we'd expect a duplicate
   symbol — this is the H0c question for *vendored* natives, and the same
   discipline (exactly one provider) is the answer. Hematite must assign the
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

**The rule (folded into D8):** every hematite-generated hosted-symbol boundary
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

## Still open in H0 (this note updated as they land)

- **H0b** — framework sysroot union (macOS `.tbd` stub union).
- **H0c** — duplicate vendored natives (two archives each bundling sqlite):
  link error, silent first-wins, or fine? Expected to need a *policy*.

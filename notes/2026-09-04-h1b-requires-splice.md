# H1b — cross-component `requires` splice

Gate H1b of [`plans/2026-09-04-trantor-v1.md`](../plans/2026-09-04-trantor-v1.md),
resolving the D18-C mechanism against the compiler. Spike: `spikes/h1b/`.
This was the adversarial review's #1 fatal risk (R7) — a driver `requires` that
names other components' nominals, on a compiler documented to segfault silently
on the failure modes. Pulled out of H7 to here.

## Base case — cross-module nominal in `requires` ✅ WORKS

A platform whose `requires { main! : {} => Widget }` references `Widget`, a
nominal `:=` owned by a **separate module** (`Widget.roc`), imported into
`main.roc` and listed in `exposes`. Builds and runs (output 42).

**So D18-C is viable:** trantor can splice the driver's `requires` verbatim as
long as every type it names is (a) imported into the generated `main.roc` and
(b) present in the generated `exposes`. The driver's `[requires.uses]` block is
exactly the list trantor needs to guarantee both.

## Probe — missing export ✅ CONFIRMS THE SEGFAULT, and the fix

Removed `Widget` from `exposes`, keeping the `import` and the `requires`
reference (the D14 failure mode: trantor gets the export list wrong).

| tool | result |
| --- | --- |
| `roc check` | clean: **"package module is private — `pf.Widget` does not name a public module"** |
| `roc build` | **SIGSEGV in the compiler**, fault address `0x138c`, no diagnostic |

This is the platform-im comment reproduced exactly. **Hard rule for trantor:
run `roc check` on the composed platform before `roc build`.** `check` turns the
compiler's silent segfault into a precise diagnostic. Equivalently, trantor can
statically assert `exposes ⊇ requires.uses` at compose time — but a `roc check`
pass is cheap insurance against this *whole class* of segfault-instead-of-error,
not just this one, so it belongs in the generated pipeline unconditionally.

## Probe — alias in `requires` ✅ CONFIRMS "nominals only"

Referenced a type **alias** (`WidgetRef : Widget`, re-exporting the nominal) in
`requires` — the shape a D13 binding module would produce if it aliased rather
than re-exported. Result: `roc check` gives 2 errors ("Type aliases … cannot
define modules"; nominal vs alias identity), and `roc build` inconsistently
reports "2 errors" yet "successfully building" a binary that then prints
`[ROC CRASHED] runtime error`. Messier than the clean segfault the docs
describe, but the same bottom line.

**Hard rule for trantor (folds into D13):** a binding module must **re-export
the original nominal**, never an alias, for any type a driver's `requires`
names. `import Widget exposing [Widget]` and pass `Widget` through — never
`WidgetRef : Widget`.

## Deferred to H3 (needs the tool)

The "renamed second component" sub-probe (compose with the `Widget` component
renamed per D14, confirm trantor rewrites `[requires.uses]` to the renamed
module) is a trantor-behavior test, not a compiler-behavior one. At the Roc
level a renamed module is just a differently-named import, which the base case
already exercises. It becomes a real assertion once H3's resolver exists.

## Net for the plan

R7 downgraded from "likeliest killer, unspiked until H7" to **"mechanism proven,
two hard rules extracted."** D18-C works. The two rules — *`roc check` before
`roc build`* and *binding modules re-export nominals, never aliases* — are now
evidence-based, not guesses.

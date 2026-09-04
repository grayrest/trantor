# H7 — roc-solid platform-im: contract-shape slice

Gate H7 of [`plans/2026-09-04-hematite-v1.md`](../plans/2026-09-04-hematite-v1.md).
Fixture: `tests/golden/imview-slice/`.

**Scope, stated plainly.** The plan's full H7 is "at least two of colorhunt/
conduit/dbx/notesviewer run on composed platforms; colorhunt's binary has no
sqlite." That is a migration of the entire immediate-mode platform — wgpu,
winit, clay layout, solid-signals, `crates/host-im`'s ~dozen subsystems — and is
the multi-week execution I flagged. This gate instead proves H7's **research
question** — the one the adversarial review called the likeliest killer (R7):
does hematite's `requires`-splice handle roc-solid's *hardest contract shape*?
It is **PARTIAL**, and the plan says so.

## What is proven, at platform-im's real contract shape

`platform-im`'s requires is `[Model : model] for main : { init, view, … }` — a
**type-parameterized** requires whose fields reference nominals owned by other
components (`Env`, `Element`, `Cmd`, `Scene`, …). It is exports-only (**zero**
hosted symbols; the host calls the app), with **multiple** provided entrypoints
(`roc_im_init`, `roc_im_view`), and the app's `Model` crosses as an opaque
`Box(Model)` while the view returns a recursive `Element` nominal.

The slice reproduces exactly that shape and **composes, typechecks, and runs**:

- Driver `imview` requires `[Model : model] for main : { init : Env -> model,
  view : model -> { tree : Element } }`, spliced verbatim by hematite (D18-C).
  `roc check` on the composed platform + app is **clean** — R7's segfault-prone
  surface, answered green.
- Two provided entrypoints (`roc_im_init`, `roc_im_view`) — the tool grew
  multi-`provides` support for this.
- It **runs headless**: the app's `init` builds a `Model` (opaque box) from
  `Env`; `view` returns `{ tree: Element }`; the host walks the recursive
  `Element` tree and prints `[hi | width-derived]`. So the `Box(Model)` +
  recursive-nominal crossing — roc-solid's characteristic ABI — works through a
  hematite composition.

Respecting a real glue hazard `platform-im` documents: the view returns an
**anonymous** record (`{ tree: Element }`), not a named nominal wrapping a
nominal (which glue silently miscompiles — "264 bytes out of bounds"). The
slice uses the anonymous form.

## Tool findings this drove

- **Multi-`provides` drivers.** `driver.toml` gained a `[[provides]]` array;
  `main.roc`'s `provides { … }` emits all entries. (A CLI driver keeps the
  single `provides_symbol`/`provided_fn` pair.)
- **Reactor drivers author their own host.** hematite's generated driver body is
  CLI-specific (it calls `roc_main`). A reactor driver (host-calls-app, custom
  render/event loop — here the `Element` renderer; in the real platform, the
  winit+wgpu+clay loop) ships its **own** `src/lib.rs`. `driver.toml`
  `authored_host = true` tells hematite to generate only the driver's
  `Cargo.toml` and never clobber the authored host. This is the honest shape of
  D5: hematite owns the *contract splice* and the runtime-provider assignment;
  the driver owns its loop.

## What a full platform-im migration still needs (the remaining ~weeks)

- The real `crates/host-im` decomposed: the engine/frame loop stays in the
  driver; `dbx.rs` (sqlite+clipboard), `docsvc.rs`, `audio.rs`, `assets.rs`,
  `eink/` become components with their own worlds. **Success criterion:
  colorhunt's binary contains no sqlite** (`nm`), the union-host disease cured.
- The wgpu/winit/clay/solid-signals rendering stack behind the driver's authored
  host (orthogonal to composition — the slice renders to text instead).
- Replacing `Request.HostOp(Str, Str)` — the stringly-typed unchecked escape
  hatch invented *because there was no composition* — with typed effect
  components. This is the north star; the slice does not yet touch it.
- `roc:test/quiesce` exercised across multiple effect sources.

The contract-shape result is the load-bearing one: the reason to fear H7 was that
the `[Model : model]` splice might segfault or fail to typecheck at real scale.
It does neither — it composes and runs. The rest is decomposition and rendering
execution over a mechanism now shown to hold.

## Exit — PARTIAL ✅

`tests/golden/imview-slice/verify.sh` green: the `[Model : model]`-parameterized,
multi-provides, exports-only reactor driver composes, typechecks, and renders a
recursive `Element` tree through a `Box(Model)` crossing. The full four-app
wgpu migration and `HostOp` elimination are documented remaining work.

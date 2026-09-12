# T3 — `trantor test <package>`

**Design log:** `notes/2026-09-12-t1-temporal-release-design-log.md`, decisions
D-T3-1 onward. Repos: `trantor` (the tool), `../trantor-temporal`,
`../trantor-net`, `../trantor-cli`.

## Why

Each package's gate is a hand-rolled `verify.sh` (495, 457 and 489 lines) that
re-implements the same composition steps — find trantor and roc, stage a world
against a sibling baseline, prove the baseline alone lacks the modules, count
`expect`s as a delta. `trantor test` already exists but only understands a
WORLD directory: it runs `roc test` on the app and `cargo test --quiet` at the
world root.

## Decisions taken (user, 2026-09-13)

1. **D-T3-1 Baseline via `[dev-deps]`.** `package.toml` gains `[dev-deps]`, same
   syntax as a world's `[deps]`, used only by `trantor test`. It never reaches
   a consumer: `deps::expand` reads `Package.deps` only.
2. **D-T3-2 README examples are built in.** `tests/readme.py` is ported to Rust.
   A package's prelude bindings live in `tests/readme-prelude.roc`, one binding
   per line; a block gets only the ones it uses and does not define.
3. **D-T3-3 Expect check is a delta > 0, no floor.** Fail only when the
   package's Roc sources contain an `expect` and it contributed none.
4. **D-T3-4 All three packages convert; every `verify.sh` is deleted.**
5. **D-T3-5 Generic core + scripts.** A check a stdout diff cannot express
   (exit codes, raw argv, peers, timing, races) is `tests/<n>/test.sh`, run by
   `trantor test`.

## Spec

`trantor test <dir>`: a `world.toml` keeps today's behaviour, unchanged. Else a
`package.toml` selects package mode, which runs, in order, stopping at the
first failure:

1. **Driver.** Add-on (no `provides_driver`): composing the package alone must
   fail, naming the missing driver. Baseline: composing alone must succeed.
2. **Composition.** A world of `[dev-deps]` + the package by path.
3. **Negative control** (add-on with dev-deps): a world of `[dev-deps]` alone
   must fail to `check` an app importing each of the package's `exports`.
4. **Expects.** `roc test` on the composed platform; with dev-deps, minus the
   dev-deps-only platform. D-T3-3.
5. **README.** When `README.md` has ```roc blocks: generate, build, run, diff.
6. **`tests/<n>/`**, sorted by name. Exactly one kind per directory:
   - `main.roc` + `expected` → compose (2), run, diff stdout.
   - `Cargo.toml` → `cargo test --release --manifest-path` it.
   - `test.sh` → run with cwd = that directory and env `TRANTOR` (this binary),
     `ROC`, `PKG` (absolute package dir), `DEPS` (the `[deps]` TOML body of
     step 2, so a script can write its own world), `TMP` (a fresh scratch dir).
   A directory matching none, or more than one, is an error.

Scratch worlds live under a temp dir: removed on success, kept and printed on
failure. Output is `ok: …` per step and `trantor test: <name> PASS`.

## Work — commit at each

1. `trantor`: `[dev-deps]` in `Package`; unit test that it does not reach
   consumers.
2. `trantor`: package mode steps 1–4 and 6 (`src/package_test.rs`).
3. `trantor`: step 5 (`src/readme_examples.rs`); unit tests for comment and
   value parsing ported from readme.py's rules.
4. `trantor`: a golden fixture for package mode — pass, plus each failure
   (wrong expected, README value, negative control) actually failing.
5. `trantor-temporal`: dev-deps, `tests/readme-prelude.roc`; delete
   `verify.sh` and `tests/readme.py`.
6. `trantor-net`: sections → `tests/`; delete `verify.sh`.
7. `trantor-cli`: sections → `tests/`; delete `verify.sh`.
8. Docs: each README says `trantor test .`; help text; design log.

For 6 and 7 the old gate is the oracle: before deleting a `verify.sh`, every
`ok:` it printed must map to a test that passes, and a sample of them must be
shown to FAIL when broken.

## Risks

- Files over 300 lines: split package mode and the README generator.
- `test.sh` must not become a way to smuggle the old gates in whole: a script
  holds one section, and any step the core now does is removed from it.
- trantor-cli is under active edit; convert it last, from a clean tree.

## What implementation changed

- Step 1's rule became "compose alone exactly when a driver is in reach through
  `[deps]`" (trantor-net), and a github dep in the chain reports the outcome
  without asserting it.
- Step 3 reads `exposes` from the composed platform instead of checking an app.
- Step 5 builds a block that opens with `app [` as a whole program.
- Scripts also get `DEV_DEPS`; script worlds must live in a directory named
  after the app header's world (`app`, `myapp`), because output is keyed by
  directory name.
- Phase 2 and 3 landed in one commit: the README step is part of the core
  sequence, so they do not build apart.

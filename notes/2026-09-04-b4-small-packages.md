# B4 — `roc:clocks` + `roc:random` + `roc:locale` + `roc:url`

Gate B4 of [`plans/2026-09-04-basic-cli-port.md`](../plans/2026-09-04-basic-cli-port.md).
Fixture: `tests/golden/b4-small/` (`verify.sh`). The owned-data packages —
no resources, no refcounted-element lists — so the interesting result is how
much of basic-cli ports **byte-verbatim**.

## Result ✅

Seven checks, all correct, exit 0: ISO-8601 year from `Utc.now!`, `Sleep`,
`Random.seed_u64!`, `Locale.get!` (`en-US` from `LANG`), `Locale.all!`,
`Locale.parse` on a runtime tag, `Url.host` of a parsed URL.

**Six basic-cli modules ship byte-identical to the 0.21 cache** (`cmp` in
`verify.sh`): `Utc.roc`, `Sleep.roc`, `Random.roc`, `Locale.roc` (324 lines
of BCP-47 parsing), `Url.roc` (1,391 lines, pure, zero host), and
`InternalDateTime.roc`. Zero edits — the pure-Roc `Host` shim (B2) just grew
six leaves. Running count of verbatim basic-cli modules: **10** (B2's four +
these six).

## Primitives (WASI boundaries)

- `roc:clocks` — `wall_now! : {} => Try(U128, [ClockBeforeEpoch])` (basic-cli's
  shape; **`U128` crosses as `u128`**, glue handles it — the H1 projection
  concern is a WIT matter only), `monotonic_now! : {} => U64`, `sleep_millis!`
  (the blocking sync form of WASI's `subscribe-duration`).
- `roc:random` — `seed_u64!`/`seed_u32!` from `/dev/urandom` (no crate dep).
- `roc:locale` (roc-native, P3) — `get!`, and **`count!`/`at!`** so the shim
  builds basic-cli's `locale_all! : () => List(Str)` in Roc (R-B5, the B2
  pattern). Tags derived from `LANGUAGE`/`LC_ALL`/`LANG` (`en_US.UTF-8` → `en-US`).
- `roc:url` — a pure package; nothing to host.

Three per-package host crates (P9): `clocks-host`, `random-host`, `locale-host`.

## Findings

1. **Binding-module names must dodge basic-cli's derived names.** A `random`
   interface whose module is `Random` would collide with basic-cli's verbatim
   `Random.roc`. Bindings are `Clocks`, `RandomHost`, `LocaleHost`; the shim
   maps `Host.random_seed_u64!` → `RandomHost.seed_u64!`. General rule for the
   port: primitive binding modules take a `*Host` suffix wherever basic-cli owns
   the plain name.
2. **`() =>` is zero-arg; `{} =>` is one unit arg** — the mirror of B2's lesson.
   basic-cli declares `now!`/`seed_u64!`/`get!`/`all!` as `() =>`, so the call
   is `Utc.now!()`, not `Utc.now!({})` ("too many args" ×4).
3. **`roc check` fails on warnings, and a constant-folded `match` is a
   warning.** `match Locale.parse("zh-Hant-TW")` is "unconditional" (known at
   compile time), `check` exits non-zero, and `build.sh`'s `set -e` stops before
   `roc build` — with no error line, so it looked like a silent build failure.
   Kept the warnings-fail discipline (roc-solid's house rule) and fixed the app:
   parse a runtime value (`Env.var!("B4_TAG") ?? "zh-Hant-TW"`). Ported example
   apps with literal inputs will hit this; the fix is always "make it runtime."

## Exit ✅

`verify.sh`: six modules `cmp`-identical to basic-cli 0.21; compose, build,
run with `LANG=en_US.UTF-8`; exact seven-line stdout; exit 0.

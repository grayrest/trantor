# H5 — seahaven migration: substitution proof (slice)

Gate H5 of [`plans/2026-09-04-hematite-v1.md`](../plans/2026-09-04-hematite-v1.md).
Fixture: `tests/golden/seahaven-slice/`.

**Scope, stated plainly.** The plan's full H5 is "decompose all of seahaven,
recompose, `rocjust` green on both a cap-std and a libc composition." Seahaven's
host is ~10.8K lines of Rust welded to the brush/cap-std sandbox and ~60 hosted
symbols; that is a multi-week migration, not a spike. This gate proves H5's two
**theses** at real-seahaven-source scale on the stdio surface, and documents the
rest as proven-pattern remaining work. It is **PARTIAL**, not complete, and the
plan says so.

## What is proven, with seahaven's real source

- **The derived layer is seahaven's actual `Stdout.roc` / `Stderr.roc`**, byte-
  identical to upstream except one mechanical edit: `import Host` → `import Stdio`
  (the migration≠composition rename, R8). `verify.sh` diffs against
  `~/dev/roc/seahaven/platform/Stdout.roc` on every run.
- **`IOErr` is seahaven's real 10-variant nominal**, copied verbatim into the
  `io` interface. It reaches the host boundary through two wrappers (`StdoutErr`,
  `StderrErr`), and glue emits it as **two structurally-identical Rust types**
  (`StdioIOErr`, `IOErr`) — the D7/D16 per-reach twin, now seen at seahaven's
  full variant set. The host constructs each; they never need to unify.
- **Substitution means something.** Two host components — `std-stdio` (real
  `std::io`) and `capture-stdio` (same code, `[cap] ` prefix) — each implement
  the same 6-symbol stdio interface. The world wires one or the other
  (`world.toml` vs `world-capture.toml`, selected with `hematite compose --world`).
  The SAME unmodified derived layer prints `out: …` under one and `[cap] out: …`
  under the other. That is the whole "alternate implementation of the CLI
  interfaces" claim, demonstrated on real code.

## Tool changes this drove

- **Symbol mangling must sanitize component names.** `std-stdio` → hosted symbol
  `hematite__std_stdio__stdout_line` (hyphen → underscore); roc rejects a hosted
  symbol that is not a valid C identifier, and cargo emits `libstd_stdio.a`, so
  both the mangled symbol and the archive filename go through `resolve::sanitize`.
- **`--world <file>`** selects the composition manifest, so one component/
  interface tree carries multiple worlds (the substitution pair).

## What a full seahaven migration still needs (the remaining ~weeks)

- The other interfaces: **filesystem** (seahaven's real 765-line `Path.roc` needs
  **20** host primitives — `file_read_bytes!`, `path_type!`, `dir_list!`,
  `path_canonicalize!`, the `file_time_*` returning **U128**, …), **process**
  (`Cmd.roc`, 499 lines), **env**, **random**, **regex**, **signal**, **tty**,
  **utc**. Each follows the exact pattern proven here: interface binding module +
  real derived layer + host impl(s).
- Wiring the real `brush-platform` cap-std sandbox as the `filesystem`/`process`
  host, versus a plain libc impl — the two-composition substitution the plan
  wants, at full surface. The stdio slice already shows two impls behind one
  interface; scaling it to fs is mechanical but large.
- `rocjust` (the real client) building and passing on the composed platform.
  Blocked until the full symbol surface is migrated.
- The `List(OsStr)` argv entrypoint (deferred since H2 on the glue-redesign
  dependency).

The **1,500-shared-lines** claim (D2) is directly supported by this slice: the
derived layer here is byte-identical to upstream and impl-independent. The fs
surface is where the bulk of those 1,500 lines live (`Path.roc` 765), and it is
the highest-value next increment.

## Exit — PARTIAL ✅

`tests/golden/seahaven-slice/verify.sh` green: seahaven's real derived layer,
two interchangeable impls, byte-identical to upstream. The substitution and
shared-derived-layer theses are proven on real code; the full ~60-symbol
migration and `rocjust` green are documented remaining work.

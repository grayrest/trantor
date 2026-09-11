# Roc `build` SIGSEGV on a type error upstream of a chained hosted effect

Found while building a drop-balance test fixture for the trantor basic-cli
port. `roc check` reports a clean type mismatch; `roc build` **segfaults** on the
same program — but only when the ill-typed value feeds a chained effectful
hosted call. This is a compiler robustness bug: `build` should surface the type
error, not crash.

## Environment

- Compiler: `Roc compiler version release-fast-84812227` (roc repo commit
  `84812227a3`, "perf(llvm): lower concat_shift_bytes to a two-register NEON
  tbl"). Reproduces with both `~/.bin/roc` and the same-commit
  `Repositories/roc/zig-out/bin/roc`.
- Host: macOS (Darwin arm64, aarch64).
- Build profile is `release-fast`, so the crash prints no stack trace
  ("Cannot print stack trace: stack tracing is disabled"). A `ReleaseSafe` /
  debug compiler build is needed to localize it.

## The program

Platform: the composed trantor basic-cli world at
`tests/golden/b8-basic-cli/platform/main.roc` (its `Cmd` is seahaven's, whose
`args : List OsStr`, where `OsStr` is a nominal type with a *string-literal*
coercion but no coercion from a runtime `Str`). The same shape exists in
seahaven's own platform. It does **not** reproduce on basic-cli 0.21 (that
`Cmd.args` types the mismatch differently and it is caught at `check`).

```roc
app [main!] { pf: platform "…/tests/golden/b8-basic-cli/platform/main.roc" }
import pf.OsStr
import pf.Stdout
import pf.Env
import pf.Cmd
main! : List(OsStr) => Try({}, _)
main! = |_args| {
	seed = Env.var_str!("SEED") ? |_| MissingSeed   # seed : Str (runtime)
	_ = Cmd.new("true").args([seed]).exec_exit_code!() ? |e| ExecFailed(e)
	Stdout.line!("ok")?
	Ok({})
}
```

The program is genuinely ill-typed: `args` wants `OsStr`, `seed` is `Str`, and a
runtime `Str` does not coerce to `OsStr` (only a `Str` *literal* does — so
`.args(["Hi"])` is well-typed and builds fine). The bug is that `build` **crashes
instead of reporting** this error.

## `roc check` reports it correctly

```
── ✗ type mismatch ──────────────────────────────────────────── rt.roc:9:27
The first argument being passed to this function has the wrong type.
	_ = Cmd.new("true").args([seed]).exec_exit_code!() ? |e| ExecFailed(e)
	                         ^^^^^^
This argument has the type:
    Str
But args needs the first argument to be:
    OsStr
```

## `roc build` crashes

```
Segmentation fault (SIGSEGV) in the Roc compiler.
Fault address: 0x12cd13a6398
Stack trace:
Cannot print stack trace: stack tracing is disabled
Please report this issue at: https://github.com/roc-lang/roc/issues
```

## Isolation (the useful part)

| variant | `roc check` | `roc build` |
|---|---|---|
| `.args([seed]).exec_exit_code!()` (runtime `seed`, chained effect) | type mismatch | **SIGSEGV** |
| `.args([seed])` with the `.exec_exit_code!()` **removed** | type mismatch | type mismatch (no crash) |
| `.args(["Hi"])` (string literal, well-typed) | ok | builds ok |
| a trivial unrelated type error (`x : Str; x = 5`) | type mismatch | type mismatch (no crash) |

So the crash is **specific**, not "build crashes on any type error":

1. It needs the **ill-typed argument** (`Str` where `OsStr` is expected inside
   the `List` element), and
2. It needs that value to be **consumed by a chained effectful hosted call**
   (`.exec_exit_code!()`). Drop the trailing effectful call and `build`
   reports the same error cleanly.

`check` never takes whatever path crashes. `build`'s extra pipeline (mono /
lambda-set specialization / codegen) appears to proceed with an ill-typed node
when the type error sits upstream of an effectful consumer in a builder chain,
then faults on the malformed IR — rather than aborting on the type error the
front end already produced.

## Suggested next steps for the compiler session

1. Rebuild the compiler in `ReleaseSafe` (or debug) and rerun `roc build` on the
   program above to get a real stack trace / panic location.
2. Confirm the divergence: `build` must be reaching a stage `check` skips while
   the module still carries a reported type error. Likely the error is not
   halting the pipeline before mono when the ill-typed value flows into an
   effectful call's argument (the `?`-desugared `exec_exit_code!` chain).
3. Minimize platform-side: the only ingredients are a nominal type (`OsStr`)
   with a literal-only `Str` coercion, a builder function taking `List` of it
   (`args`), and an effectful hosted function consuming the builder. A ~30-line
   hand-rolled platform should reproduce without trantor/seahaven.

## Impact

Low for correct programs (well-typed code is unaffected), but it turns an
ordinary type error into a crash with no source location during `build`, which
is a bad first-run experience — the error only becomes legible if you happen to
run `roc check` separately.

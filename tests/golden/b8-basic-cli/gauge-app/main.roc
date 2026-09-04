app [main!] { pf: platform "../platform/main.roc" }
import pf.OsStr
import pf.Stdout
import pf.Env
import pf.Path

## Drop-balance gauge fixture. $GAUGE_SEED (a >23-byte string set by verify.sh)
## is concatenated at RUNTIME so `big` is a real HEAP RocStr, not a static
## literal -- only a heap allocation is visible to the gauge. It crosses into
## the host through Env.set_cwd! -> Host.env_set_cwd! -> FsOps.set_cwd! ->
## Cell.put!, an owned-RocStr host argument (B0). Under HEMATITE_ALLOC_GAUGE the
## driver prints the alloc/dealloc balance at exit; a host that fails to release
## this owned arg leaks `big` and shows live>0 (verified: it does).
##
## NOTE: the subprocess/http element-list release paths (also B0-fixed) are not
## gauged here -- the pinned compiler segfaults when a runtime Str is passed to
## Cmd.args, and small-string (inline) args don't heap-allocate, so there is no
## heap element for a gauge to catch. Those fixes rest on the glue's documented
## whole-struct decref instead.
main! : List(OsStr) => Try({}, _)
main! = |_args| {
	seed = Env.var_str!("GAUGE_SEED") ? |_| MissingGaugeSeed
	big = Str.concat("/tmp/gauge-cwd-", seed)
	Env.set_cwd!(Path.from_raw(Utf8(big))) ? |e| SetCwdFailed(e)
	Stdout.line!("gauge-ran")?
	Ok({})
}

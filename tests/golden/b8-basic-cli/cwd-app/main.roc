app [main!] { pf: platform "../platform/main.roc" }
import pf.OsStr
import pf.Stdout
import pf.Env
import pf.Path
import pf.Cmd

## Proves Env.set_cwd! propagates to the subprocess working directory (Option A
## cwd model): before set_cwd! a child runs in the process cwd; after, it runs
## in the userland cwd the platform holds -- without mutating this process's
## real cwd. `pwd` prints the child's directory.
main! : List(OsStr) => Try({}, _)
main! = |_args| {
	before = Cmd.new("pwd").exec_output!() ? |e| ExecBeforeFailed(e)
	Stdout.line!(Str.concat("before: ", Str.trim(before.stdout_utf8)))?
	target : Path
	target = "/usr"
	Env.set_cwd!(target) ? |e| SetCwdFailed(e)
	after = Cmd.new("pwd").exec_output!() ? |e| ExecAfterFailed(e)
	Stdout.line!(Str.concat("after: ", Str.trim(after.stdout_utf8)))?
	Ok({})
}

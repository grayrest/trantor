app [main!] { pf: platform "../platform/main.roc" }
import pf.Stdout
import pf.Cmd

## seahaven-shaped subprocess use: PATH-searched exec with captured output,
## an exit code, and check_available!.
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	echo = Cmd.args_str(Cmd.new_str("echo"), ["hello", "from", "a", "child"])
	out = match Cmd.exec_output!(echo) {
		Ok(o) => o.stdout_utf8
		Err(_) => "<exec failed>\n"
	}
	Stdout.write!(Str.concat("output: ", out)) ?? {}
	code = Cmd.exec_exit_code!(Cmd.args_str(Cmd.new_str("sh"), ["-c", "exit 7"])) ?? -1
	Stdout.line!(Str.concat("exit-code: ", if code == 7 { "7" } else { "unexpected" })) ?? {}
	avail = if Cmd.check_available!("ls") { "ok" } else { "missing" }
	Stdout.line!(Str.concat("path-search: ", avail)) ?? {}
	Ok({})
}

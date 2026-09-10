app [main!] { pf: platform "../target/hematite/b2-cli/platform/main.roc" }
import pf.Stdout
import pf.Stderr
import pf.Env

## A basic-cli program, unchanged: main! takes the args list.
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |args| {
	Stdout.line!("hello from basic-cli on hematite") ?? {}
	Stderr.line!("(diagnostic on stderr)") ?? {}
	name = Env.var!("USER") ?? "stranger"
	Stdout.line!(Str.concat("user: ", name)) ?? {}
	Stdout.line!(Str.concat("args: ", Str.join_with(args, ","))) ?? {}
	Ok({})
}

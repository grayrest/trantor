app [run!] { pf: platform "../target/trantor/b2-cli/platform/main.roc" }
import pf.Stdout
import pf.Stderr
import pf.Env

## The same program in the WASI-shaped run! form (P8): no params, args via a call.
run! : {} => Try({}, [Exit(I32), ..])
run! = |{}| {
	Stdout.line!("hello from basic-cli on trantor") ?? {}
	Stderr.line!("(diagnostic on stderr)") ?? {}
	name = Env.var!("USER") ?? "stranger"
	Stdout.line!(Str.concat("user: ", name)) ?? {}
	Stdout.line!(Str.concat("args: ", Str.join_with(Env.args!({}), ","))) ?? {}
	Ok({})
}

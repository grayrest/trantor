app [main!] { pf: platform "../target/hematite/seahaven-slice/platform/main.roc" }
import pf.Stdout
import pf.Stderr
## Exercises seahaven's REAL Stdout/Stderr derived layer over a hematite-composed
## stdio interface.
main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Stdout.line!("out: hello from a composed seahaven slice") ?? {}
	Stderr.line!("err: diagnostics here") ?? {}
	Stdout.write!("no-newline") ?? {}
	Stdout.line!("") ?? {}
	Ok({})
}

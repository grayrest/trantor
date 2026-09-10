app [main!] { pf: platform "../target/hematite/b8-basic-cli/platform/main.roc" }
import pf.OsStr
import pf.Stdout
import pf.Path

## Writes outside the cwd. The unconfined baseline allows it (ambient parity);
## the confined world's roc:filesystem impl refuses it (seahaven-as-component).
main! : List(OsStr) => Try({}, _)
main! = |_args| {
	target : Path
	target = "../b8-escape.txt"
	verdict = match target.write_utf8!("escaped") {
		Ok({}) => {
			_ = target.delete!()
			"allowed"
		}
		Err(PathErr(PermissionDenied)) => "denied"
		Err(_) => "error"
	}
	Stdout.line!("escape: ${verdict}")?
	Ok({})
}

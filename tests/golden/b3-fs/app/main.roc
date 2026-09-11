app [main!] { pf: platform "../target/trantor/b3-fs/platform/main.roc" }
import pf.Stdout
import pf.Path
import pf.Env

## basic-cli-shaped file program. Exit code == live descriptors at the end
## (0 = every Descriptor dropped). The `escape:` line is the capability probe:
## unconfined reads /etc/hosts, confined is denied.
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	dir = Path.from_str("b3-scratch")
	Path.create_all!(dir) ?? {}
	f = Path.join(dir, "note.txt")
	Path.write_utf8!(f, "one\ntwo\n") ?? {}
	txt = Path.read_utf8!(f) ?? "<read failed>"
	Stdout.write!("read: ") ?? {}
	Stdout.write!(txt) ?? {}
	kind = if Path.is_file!(f) ?? False { "file" } else { "not-file" }
	Stdout.line!(Str.concat("kind: ", kind)) ?? {}
	names = Path.list!(dir) ?? []
	Stdout.line!(Str.concat("list: ", Str.join_with(names.map(Path.display), ","))) ?? {}
	cwd = Env.cwd!({}) ?? Path.from_str("?")
	Stdout.line!(Str.concat("cwd-ok: ", if Str.ends_with(Path.display(cwd), "b3-fs") { "yes" } else { "no" })) ?? {}
	esc = match Path.read_bytes!(Path.from_str("/etc/hosts")) {
		Ok(_) => "escaped"
		Err(_) => "denied"
	}
	Stdout.line!(Str.concat("escape: ", esc)) ?? {}
	Path.delete!(f) ?? {}
	Path.delete_empty!(dir) ?? {}
	Err(Exit(Path.live!({})))
}

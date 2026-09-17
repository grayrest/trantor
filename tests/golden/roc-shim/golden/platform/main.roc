platform ""
	requires {
		main! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Path, Stdio, Env]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"trantor__cell__put": Cell.put!,
		"trantor__cell__get": Cell.get!,
		"trantor__env__path_arg": Env.path_arg!,
		"trantor__stdio__stdout_line": Stdio.line!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libcli.a", "libcell.a", "libenv.a", "libstdio.a", app] },
		x64mac: { inputs: ["libcli.a", "libcell.a", "libenv.a", "libstdio.a", app] },
		arm64glibc: { inputs: ["Scrt1.o", "crti.o", "libcli.a", "libcell.a", "libenv.a", "libstdio.a", app, "crtn.o", "libc.so", "libm.so.6", "libgcc_s.so.1"] },
		x64glibc: { inputs: ["Scrt1.o", "crti.o", "libcli.a", "libcell.a", "libenv.a", "libstdio.a", app, "crtn.o", "libc.so", "libm.so.6", "libgcc_s.so.1"] },
	}

import Cell
import Env
import Stdio
import Path
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!({}) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

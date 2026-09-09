platform ""
	requires {
		main! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Path, Stdio, Env]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__cell__put": Cell.put!,
		"hematite__cell__get": Cell.get!,
		"hematite__env__path_arg": Env.path_arg!,
		"hematite__stdio__stdout_line": Stdio.line!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libcli.a", "libcell.a", "libenv.a", "libstdio.a", app] },
		x64mac: { inputs: ["libcli.a", "libcell.a", "libenv.a", "libstdio.a", app] },
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

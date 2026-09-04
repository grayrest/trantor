platform ""
	requires {
		main! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Path, Stdio, Env]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__stdio__stdout_line": Stdio.line!,
		"hematite__audit__file_read": Fs.file_read!,
		"hematite__env__path_arg": Env.path_arg!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libstdio.a", "libcapstdfs.a", "libaudit.a", "libenv.a", "libcli.a", app] },
		x64mac: { inputs: ["libstdio.a", "libcapstdfs.a", "libaudit.a", "libenv.a", "libcli.a", app] },
	}

import Stdio
import Fs
import Path
import Env
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!({}) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

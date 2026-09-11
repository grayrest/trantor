platform ""
	requires {
		main! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Path, Stdio, Env]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"trantor__env__path_arg": Env.path_arg!,
		"trantor__audit__file_read": Fs.file_read!,
		"trantor__stdio__stdout_line": Stdio.line!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libcli.a", "libcapstdfs.a", "libenv.a", "libstdio.a", "libaudit.a", app] },
		x64mac: { inputs: ["libcli.a", "libcapstdfs.a", "libenv.a", "libstdio.a", "libaudit.a", app] },
	}

import Env
import Fs
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

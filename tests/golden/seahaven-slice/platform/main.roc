platform ""
	requires {
		main! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Stdout, Stderr]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__std_stdio__stdout_line": Stdio.stdout_line!,
		"hematite__std_stdio__stdout_write": Stdio.stdout_write!,
		"hematite__std_stdio__stdout_write_bytes": Stdio.stdout_write_bytes!,
		"hematite__std_stdio__stderr_line": Stdio.stderr_line!,
		"hematite__std_stdio__stderr_write": Stdio.stderr_write!,
		"hematite__std_stdio__stderr_write_bytes": Stdio.stderr_write_bytes!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libcli.a", "libstd_stdio.a", app] },
		x64mac: { inputs: ["libcli.a", "libstd_stdio.a", app] },
	}

import Stdio
import Stdout
import Stderr
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!({}) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

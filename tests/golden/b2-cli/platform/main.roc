platform ""
	requires {
		main! : List(Str) => Try({}, [Exit(I32), ..])
	}
	exposes [Stdout, Stderr, Stdin, Tty, Env]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__cli_host__arg_count": CliEnv.arg_count!,
		"hematite__cli_host__arg_at": CliEnv.arg_at!,
		"hematite__cli_host__var": CliEnv.var!,
		"hematite__cli_host__env_count": CliEnv.env_count!,
		"hematite__cli_host__env_at": CliEnv.env_at!,
		"hematite__cli_host__platform": CliEnv.platform!,
		"hematite__cli_host__get_stdin": CliIn.get_stdin!,
		"hematite__cli_host__read_line": CliIn.read_line!,
		"hematite__cli_host__read_to_end": CliIn.read_to_end!,
		"hematite__cli_host__get_stdout": CliOut.get_stdout!,
		"hematite__cli_host__get_stderr": CliOut.get_stderr!,
		"hematite__cli_host__enable_raw_mode": CliTty.enable_raw_mode!,
		"hematite__cli_host__disable_raw_mode": CliTty.disable_raw_mode!,
		"hematite__sync_io__read": Streams.read!,
		"hematite__sync_io__write": Streams.write!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libmain_driver.a", "libsync_io.a", "libcli_host.a", app] },
		x64mac: { inputs: ["libmain_driver.a", "libsync_io.a", "libcli_host.a", app] },
	}

import CliEnv
import CliIn
import CliOut
import CliTty
import Streams
import Stdout
import Stderr
import Stdin
import Tty
import Env
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!(Env.args!({})) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

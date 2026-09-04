platform ""
	requires {
		run! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Streams, Memory, FileIo]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__file__open": FileIo.open!,
		"hematite__memory__open": Memory.open!,
		"hematite__memory__report": Memory.report!,
		"hematite__sync_io__read": Streams.read!,
		"hematite__sync_io__write": Streams.write!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libsync_io.a", "libfile.a", "libmemory.a", "libcli.a", app] },
		x64mac: { inputs: ["libsync_io.a", "libfile.a", "libmemory.a", "libcli.a", app] },
	}

import FileIo
import Memory
import Streams
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match run!({}) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

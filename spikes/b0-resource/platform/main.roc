platform ""
	requires {
		run! : {} => Try({}, [Exit(I32), ..])
	}
	exposes [Gauge]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__gauge__open": Gauge.open!,
		"hematite__gauge__bump": Gauge.bump!,
		"hematite__gauge__report": Gauge.report!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libgauge.a", "libcli.a", app] },
		x64mac: { inputs: ["libgauge.a", "libcli.a", app] },
	}

import Gauge

main_for_host! : () => I32
main_for_host! = || {
	match run!({}) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

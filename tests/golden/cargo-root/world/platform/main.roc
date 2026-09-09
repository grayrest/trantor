platform ""
	requires {
		main! : {} => I32
	}
	exposes [Host]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__svc__seed": Host.seed!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libdrv.a", "libsvc.a", app] },
		x64mac: { inputs: ["libdrv.a", "libsvc.a", app] },
	}

import Host

main_for_host! : () => I32
main_for_host! = || main!({})

platform ""
	requires { main! : {} => I64 }
	exposes [Effect]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__a__ping": Host.ping!,
		"hematite__b__boom": Host.boom!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["liba.a", "libb.a", app] },
		x64mac: { inputs: ["liba.a", "libb.a", app] },
	}

import Host
import Effect

main_for_host! : () => I64
main_for_host! = || main!({})

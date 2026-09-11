platform ""
	requires { main! : {} => I64 }
	exposes [Effect]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"trantor__a__ping": Host.ping!,
		"trantor__b__pong": Host.pong!,
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

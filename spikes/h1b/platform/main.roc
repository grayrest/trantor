platform ""
	requires { main! : {} => WidgetRef }
	exposes [Widget, WidgetRef, Effect]
	packages {}
	provides { "roc_main": main_for_host! }
	hosted {
		"trantor__a__ping": Host.ping!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["liba.a", app] },
		x64mac: { inputs: ["liba.a", app] },
	}

import Host
import Effect
import Widget exposing [Widget]
import WidgetRef exposing [WidgetRef]

main_for_host! : () => I64
main_for_host! = || {
	w = main!({})
	Widget.value(w)
}

platform ""
	requires {
		main! : {} => { message : Str, n : I64 }
	}
	exposes [Host]
	packages {}
	provides { "roc_app": app_for_host! }
	hosted {
		"hematite__b__seed": Host.seed!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["liba.a", "libb.a", app] },
		x64mac: { inputs: ["liba.a", "libb.a", app] },
		wasm32: { inputs: ["host.wasm", app], output: Shared, exports: ["wasm_main", "wasm_result_len", "wasm_n", "wasm_alloc_count", "wasm_release"] },
	}

import Host

app_for_host! : {} => { message : Str, n : I64 }
app_for_host! = |{}| main!({})

platform ""
    requires {} {
        main! : {} => {
            message : Str,
            n : I64,
        }
    }
    exposes [Host]
    packages {}
    provides {
        "roc_app": app_for_host!,
    }
    hosted {
        "hematite__b__seed": Host.seed!,
    }
    targets: {
        inputs_dir: "targets/",
        ## ONE merged host.wasm (see build.sh for the two measured negatives:
        ## roc links wasm inputs --whole-archive, so per-component inputs
        ## collide on std / compiler-builtins whatever their form).
        wasm32: { inputs: ["host.wasm", app], output: Shared, exports: ["wasm_main", "wasm_result_len", "wasm_n", "wasm_alloc_count", "wasm_release"] },
    }

import Host

app_for_host! = |{}| main!({})

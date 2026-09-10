app [main!] { pf: platform "../target/hematite/wasm-host/platform/main.roc" }

import pf.Host

## roc-solid's wasm gate-zero app: a runtime seed from the host (proving the
## hosted-call direction), a string built from it (proving roc_alloc runs on
## wasm32), and a scalar beside it.
main! = |{}| {
	message: "hematite wasm host seed=${Host.seed!({}).to_str()}",
	n: Host.seed!({}) * 2,
}

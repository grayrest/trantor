app [main!] { pf: platform "../platform/main.roc" }

import pf.Host

## Mirrors roc-solid's wasm gate-zero app; `Host.seed!` is now implemented in a
## SECOND wasm input (component b), so the link resolves app -> b -> a.
main! = |{}| {
    message: "hematite wasm inputs seed=${Host.seed!({}).to_str()}",
    n: Host.seed!({}) * 2,
}

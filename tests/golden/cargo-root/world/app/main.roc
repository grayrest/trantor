app [main!] { pf: platform "../../target/trantor/world/platform/main.roc" }

import pf.Host

## Exit code = the seed component b answers with (21).
main! = |{}| Host.seed!({})

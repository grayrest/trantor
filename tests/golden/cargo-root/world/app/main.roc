app [main!] { pf: platform "../platform/main.roc" }

import pf.Host

## Exit code = the seed component b answers with (21).
main! = |{}| Host.seed!({})

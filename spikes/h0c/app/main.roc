app [main!] { pf: platform "../platform/main.roc" }

import pf.Effect

main! : {} => I64
main! = |{}| {
	a = Effect.ping!({})
	b = Effect.pong!({})
	a + b
}

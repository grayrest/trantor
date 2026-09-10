app [Model, main] { pf: platform "../target/hematite/imview-slice/platform/main.roc" }

import pf.Element exposing [Element]
import pf.Env exposing [Env]

Model : { label : Str, n : U64 }

main = {
	init: |env| { label: "hi", n: Env.width(env) },
	view: |model| {
		tree: Element.row([
			Element.text(model.label),
			Element.text("width-derived"),
		]),
	},
}

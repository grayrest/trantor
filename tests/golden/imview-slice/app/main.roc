app [Model, main] { pf: platform "../target/trantor/imview-slice/platform/main.roc" }

import pf.Element
import pf.Env

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

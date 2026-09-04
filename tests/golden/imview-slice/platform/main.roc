platform ""
	requires {
		[Model : model] for main : {
			init : Env -> model,
			view : model -> { tree : Element },
		}
	}
	exposes [Element, Env]
	packages {}
	provides {
		"roc_im_init": init_for_host,
		"roc_im_view": view_for_host,
	}
	hosted {
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libimview.a", app] },
		x64mac: { inputs: ["libimview.a", app] },
	}

import Element
import Env

init_for_host : Env -> Box(Model)
init_for_host = |env| {
	init_fn = main.init
	Box.box(init_fn(env))
}

view_for_host : Box(Model) -> { tree : Element }
view_for_host = |boxed_model| {
	view_fn = main.view
	view_fn(Box.unbox(boxed_model))
}

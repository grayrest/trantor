platform ""
	requires {
		[Model : model] for main : {
			init : Env -> model,
			view : model, Env -> { tree : Element },
			cmds : model -> List(Cmd),
			on_event : Event, model -> model,
		}
	}
	exposes [Element, Env, Cmd, Event, Echo, EchoEvent, Tick, TickEvent, TickEnv]
	packages {}
	provides {
		"roc_im_init": init_for_host,
		"roc_im_view": view_for_host,
		"roc_im_cmds": cmds_for_host,
		"roc_im_route": route_for_host,
	}
	hosted {
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libimview.a", "libsvc_echo.a", "libsvc_tick.a", app] },
		x64mac: { inputs: ["libimview.a", "libsvc_echo.a", "libsvc_tick.a", app] },
	}

import Element
import Env
import Cmd
import Event
import Echo
import EchoEvent
import Tick
import TickEvent
import TickEnv

init_for_host : Env -> Box(Model)
init_for_host = |env| {
	init_fn = main.init
	Box.box(init_fn(env))
}

view_for_host : Box(Model), Env -> { tree : Element }
view_for_host = |boxed_model, env| {
	view_fn = main.view
	view_fn(Box.unbox(boxed_model), env)
}

## Returns `List(Cmd)` — the nested-union wrapper crossing OUT of Roc (R-H7-1).
cmds_for_host : Box(Model) -> List(Cmd)
cmds_for_host = |boxed_model| {
	cmds_fn = main.cmds
	cmds_fn(Box.unbox(boxed_model))
}

## Takes an `Event` — the nested-union wrapper crossing INTO Roc (R-H7-1).
route_for_host : Box(Model), Event -> Box(Model)
route_for_host = |boxed_model, event| {
	on_event_fn = main.on_event
	Box.box(on_event_fn(event, Box.unbox(boxed_model)))
}

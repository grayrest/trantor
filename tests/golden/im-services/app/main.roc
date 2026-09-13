app [Model, main] { pf: platform "../target/trantor/im-services/platform/main.roc" }

import pf.Element exposing [Element]
import pf.Env exposing [Env]
import pf.Cmd exposing [Cmd]
import pf.Event exposing [Event]
import pf.Echo
import pf.EchoEvent
import pf.Tick
import pf.TickEvent
import pf.Bell
import pf.BellEvent
import pf.Nudge
import pf.NudgeEvent

Model : { label : Str, log : List(Str), outbox : List(Cmd) }

main = {
	init: |_env| {
		label: "hi",
		log: [],
		outbox: [
			Cmd.Log("init"),
			Cmd.Echo(Echo.Ping(0, "echo-key", "hello")),
			Cmd.Tick(Tick.Start(0, "tick-key", 3)),
			Cmd.Bell(Bell.Ring("bell-key", 9, "ding, in a string long enough to live on the heap")),
			Cmd.Nudge(Nudge.Poke),
		],
	},
	view: |model, env| {
		ticks = Env.ticks(env)
		{
			tree: Element.row(
				List.concat(
					[Element.text(model.label)],
					List.append(List.map(model.log, Element.text), Element.text("ticks=${ticks.to_str()}")),
				),
			),
		}
	},
	cmds: |model| model.outbox,
	on_event: |event, model| {
		line = match event {
			Event.Echo(EchoEvent.Pong(s)) => "pong:${s}"
			Event.Echo(EchoEvent.Shouted(s, n)) => "shouted:${s}:${n.to_str()}"
			Event.Tick(TickEvent.Ticked(n)) => "tick:${n.to_str()}"
			Event.Tick(TickEvent.Stopped) => "stopped"
			Event.Bell(BellEvent.Rang(s, n)) => "rang:${s}:${n.to_str()}"
			Event.Nudge(NudgeEvent.Poked(s)) => s
		}
		{ ..model, log: List.append(model.log, line), outbox: [] }
	},
}

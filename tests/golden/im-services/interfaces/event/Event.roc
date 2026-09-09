## The composed event union (see Cmd.roc).
import EchoEvent
import TickEvent

Event := [
	Echo(EchoEvent),
	Tick(TickEvent),
]

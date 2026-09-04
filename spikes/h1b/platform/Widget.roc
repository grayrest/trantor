## A nominal type owned by the "widgets" component. The driver's `requires`
## references it by name -- the cross-component reference H1b probes.
Widget := [W(I64)].{
	make : I64 -> Widget
	make = |n| Widget.W(n)

	value : Widget -> I64
	value = |w| {
		match w {
			Widget.W(n) => n
		}
	}
}

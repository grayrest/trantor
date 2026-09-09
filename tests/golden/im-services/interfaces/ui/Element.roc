## A recursive UI element tree -- the roc-solid Element shape, minimized.
Element := [
	Text(Str),
	Row(List(Element)),
].{
	text : Str -> Element
	text = |s| Element.Text(s)

	row : List(Element) -> Element
	row = |kids| Element.Row(kids)
}

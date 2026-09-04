app [main!] { pf: platform "../platform/main.roc" }
import pf.Widget exposing [Widget]
import pf.WidgetRef exposing [WidgetRef]
main! : {} => WidgetRef
main! = |{}| Widget.make(42)

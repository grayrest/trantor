app [main!] { pf: platform "../target/trantor/app/platform/main.roc" }

import pf.OsStr
import pf.Stdout
import pf.Greet

main! : List(OsStr) => Try({}, _)
main! = |args| Stdout.line!(Greet.hello(if List.len(args) > 99 { "many" } else { "app" }))

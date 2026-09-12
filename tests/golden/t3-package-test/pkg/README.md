# greet

```roc
import pf.Greet

Greet.hello(who)   # "hello world"
```

A whole program:

```roc
app [main!] { pf: platform "../target/trantor/myapp/platform/main.roc" }

import pf.OsStr
import pf.Stdout
import pf.Greet

main! : List(OsStr) => Try({}, _)
main! = |_args| Stdout.line!(Greet.hello("reader"))
```

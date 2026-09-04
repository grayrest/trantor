import Stdout
## A pure-Roc extension module: composes existing platform modules only.
Greeting :: [].{
	hello! : Str => Try({}, [StdoutErr(IOErr), ..])
	hello! = |name| Stdout.line!("Hello, ${name}!")
}

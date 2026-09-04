import Host

## App-facing wrappers over the two hosted symbols, each implemented in a
## separate archive (comp-a defines ping, comp-b defines pong).
Effect :: [].{
	ping! : {} => I64
	ping! = |{}| Host.ping!({})

	pong! : {} => I64
	pong! = |{}| Host.pong!({})
}

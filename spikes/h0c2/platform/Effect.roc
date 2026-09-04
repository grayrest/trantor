import Host
Effect :: [].{ ping! : {} => I64
	ping! = |{}| Host.ping!({}) }

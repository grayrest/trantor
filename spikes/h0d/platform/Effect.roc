import Host
Effect :: [].{
	ping! : {} => I64
	ping! = |{}| Host.ping!({})
	boom! : {} => I64
	boom! = |{}| Host.boom!({})
}

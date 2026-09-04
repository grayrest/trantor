import IOErr exposing [IOErr]
## roc:random/random. Named RandomHost so basic-cli's derived `Random.roc` keeps its name.
RandomHost :: [].{
	seed_u64! : {} => Try(U64, [RandomErr(IOErr)])
	seed_u32! : {} => Try(U32, [RandomErr(IOErr)])
}

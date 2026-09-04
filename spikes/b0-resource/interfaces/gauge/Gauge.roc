## B0 drop-balance gauge. `Counter` is a resource: a refcounted opaque host
## handle spelled exactly as basic-cli spells FileReader/TcpStream.
Gauge :: [].{
	Counter :: Box(U64)
	open! : {} => Counter
	bump! : Counter => U64
	## Host closes observed so far (the destructor count). Returns I32 so the
	## app can surface it as the exit code.
	report! : {} => I32
}

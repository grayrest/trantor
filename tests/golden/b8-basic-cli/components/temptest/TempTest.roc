import TestNet

## TEMP TESTING namespace — NOT part of the platform's real surface. It bundles
## the throwaway test scaffolding (here: the in-process HTTP test server the
## http examples talk to) under one clearly-named module, so it doesn't sit in
## `exposes` looking like basic-cli API. Forwards to the internal `testnet`
## interface. To be replaced when a real http serving API is built.
TempTest :: [].{
	start_test_server! : {} => {}
	start_test_server! = |_| TestNet.start_test_server!({})
}

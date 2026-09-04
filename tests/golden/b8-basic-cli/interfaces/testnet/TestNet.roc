## roc:testnet — TEST-ONLY. Starts the in-process HTTP test server (:9000)
## the basic-cli http examples expect (basic-cli runs ci/rust_http_server; here
## it is a host thread). Binds synchronously, then serves on a background thread.
TestNet :: [].{
	start_test_server! : {} => {}
}

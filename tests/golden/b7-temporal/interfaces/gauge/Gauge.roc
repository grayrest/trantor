## How many host resources are alive right now. 0 at the end of a run means
## Roc dropped every handle and the B0 registry destructed them.
Gauge :: [].{
	live! : {} => I32
}

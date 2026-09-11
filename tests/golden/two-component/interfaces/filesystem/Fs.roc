## trantor-generated binding module for wiring point `fs`.
## Chain: fs = audit(capstdfs). App-facing head is audit; capstdfs is reached
## host-internally (audit's Rust calls capstdfs's Rust), so only the head is a
## Roc hosted leaf here.
import IOErr exposing [IOErr]
Fs :: [].{
	file_read! : Str => Try(Str, [FileErr(IOErr)])
}

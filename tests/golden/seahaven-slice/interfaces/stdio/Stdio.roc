import IOErr exposing [IOErr]
## hematite binding module for interface roc:cli/stdio. The six primitive
## leaves seahaven's real Stdout/Stderr derived layer wraps.
Stdio :: [].{
	stdout_line! : Str => Try({}, [StdoutErr(IOErr)])
	stdout_write! : Str => Try({}, [StdoutErr(IOErr)])
	stdout_write_bytes! : List(U8) => Try({}, [StdoutErr(IOErr)])
	stderr_line! : Str => Try({}, [StderrErr(IOErr)])
	stderr_write! : Str => Try({}, [StderrErr(IOErr)])
	stderr_write_bytes! : List(U8) => Try({}, [StderrErr(IOErr)])
}

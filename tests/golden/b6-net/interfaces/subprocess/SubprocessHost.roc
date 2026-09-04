import IOErr exposing [IOErr]
## roc:subprocess (roc-native, P3; seahaven's design). The crossing record
## keeps seahaven's NativeOsStr union so Cmd.roc's host seam and PATH-split
## match port verbatim; the host handles Utf8 (the only variant Roc mints
## here) and UnixBytes (raw). Outputs are byte lists + exit code.
SubprocessHost :: [].{
	NativeOsStr : [Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]
	Cmd : { args : List(NativeOsStr), clear_envs : Bool, envs : List(NativeOsStr), program : NativeOsStr }
	CmdOutputSuccess : { stderr_bytes : List(U8), stdout_bytes : List(U8) }
	CmdOutputFailure : { stderr_bytes : List(U8), stdout_bytes : List(U8), exit_code : I32 }
	exec_exit_code! : Cmd => Try(I32, IOErr)
	exec_status! : Cmd => Try(I32, IOErr)
	exec_output! : Cmd => Try(CmdOutputSuccess, [NonZeroExitCode(CmdOutputFailure), FailedToGetExitCode(IOErr)])
	exec_output_inherit_stdin! : Cmd => Try(CmdOutputSuccess, [NonZeroExitCode(CmdOutputFailure), FailedToGetExitCode(IOErr)])
}

import IOErr exposing [IOErr]
## roc:cli/environment primitives. Lists arrive as count+at so the derived
## layer builds List(Str) in Roc (the pinned glue has no host-side
## RocList<RocStr> constructor, R-B5). Str-based per P11 (no OsStr).
CliEnv :: [].{
	arg_count! : {} => U64
	arg_at! : U64 => Str
	var! : Str => Try(Str, [VarNotFound(Str), EnvErr(IOErr)])
	env_count! : {} => U64
	env_at! : U64 => { name : Str, value : Str }
	cwd! : {} => Str
	exe_path! : {} => Str
	temp_dir! : {} => Str
	platform! : {} => { arch : [X86, X64, ARM, AARCH64, OTHER(Str)], os : [LINUX, MACOS, WINDOWS, OTHER(Str)] }
}

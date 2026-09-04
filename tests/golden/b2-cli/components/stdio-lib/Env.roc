## Str-based Env (P11: OsStr dropped). basic-cli's Env.roc is built on
## OsStr/Path, so its var/dict/platform/args surface is re-expressed over Str;
## cwd!/set_cwd!/exe_path!/temp_dir! (Path-typed) arrive with roc:path in B3.
import IOErr exposing [IOErr]
import Host
import CliEnv
Env :: [].{
	## Command-line arguments (program name excluded), collected in Roc.
	args! : {} => List(Str)
	args! = |{}| collect!(CliEnv.arg_count!({}), 0, [], |i| CliEnv.arg_at!(i))

	var! : Str => Try(Str, [VarNotFound(Str), EnvErr(IOErr), ..])
	var! = |name| widen_var(Host.env_var!(name))

	dict! : {} => List({ name : Str, value : Str })
	dict! = |{}| collect!(CliEnv.env_count!({}), 0, [], |i| CliEnv.env_at!(i))

	platform! : () => { arch : [X86, X64, ARM, AARCH64, OTHER(Str)], os : [LINUX, MACOS, WINDOWS, OTHER(Str)] }
	platform! = || Host.env_platform!()
}

collect! : U64, U64, List(a), (U64 => a) => List(a)
collect! = |n, i, acc, get!| if i >= n { acc } else { collect!(n, i + 1, List.append(acc, get!(i)), get!) }

widen_var : Try(a, [VarNotFound(Str), EnvErr(IOErr)]) -> Try(a, [VarNotFound(Str), EnvErr(IOErr), ..])
widen_var = |r| match r {
	Ok(v) => Ok(v)
	Err(VarNotFound(n)) => Err(VarNotFound(n))
	Err(EnvErr(e)) => Err(EnvErr(e))
}

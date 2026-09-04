## Str-based Env (P11). B3 restores the Path-typed surface: cwd!/set_cwd! are
## the userland cwd over preopens (P8), exe_path!/temp_dir! are Str from the host.
import IOErr exposing [IOErr]
import Host
import CliEnv
import FsOps
import Path exposing [Path]
Env :: [].{
	args! : {} => List(Str)
	args! = |{}| collect!(CliEnv.arg_count!({}), 0, [], |i| CliEnv.arg_at!(i))
	var! : Str => Try(Str, [VarNotFound(Str), EnvErr(IOErr), ..])
	var! = |name| widen_var(Host.env_var!(name))
	dict! : {} => List({ name : Str, value : Str })
	dict! = |{}| collect!(CliEnv.env_count!({}), 0, [], |i| CliEnv.env_at!(i))
	platform! : () => { arch : [X86, X64, ARM, AARCH64, OTHER(Str)], os : [LINUX, MACOS, WINDOWS, OTHER(Str)] }
	platform! = || Host.env_platform!()
	cwd! : {} => Try(Path, [CwdUnavailable, ..])
	cwd! = |{}| Ok(Path.from_str(FsOps.cwd!({})))
	set_cwd! : Path => Try({}, [InvalidCwd(IOErr), ..])
	set_cwd! = |p| {
		FsOps.set_cwd!(Path.to_str(p))
		Ok({})
	}
	exe_path! : {} => Try(Path, [ExePathUnavailable, ..])
	exe_path! = |{}| Ok(Path.from_str(CliEnv.exe_path!({})))
	temp_dir! : {} => Path
	temp_dir! = |{}| Path.from_str(CliEnv.temp_dir!({}))
}

collect! : U64, U64, List(a), (U64 => a) => List(a)
collect! = |n, i, acc, get!| if i >= n { acc } else { collect!(n, i + 1, List.append(acc, get!(i)), get!) }

widen_var : Try(a, [VarNotFound(Str), EnvErr(IOErr)]) -> Try(a, [VarNotFound(Str), EnvErr(IOErr), ..])
widen_var = |r| match r {
	Ok(v) => Ok(v)
	Err(VarNotFound(n)) => Err(VarNotFound(n))
	Err(EnvErr(e)) => Err(EnvErr(e))
}

## The basic-cli `Host` surface, reconstructed in pure Roc over the WASI-shaped
## primitives (P4/D2). basic-cli's own Stdout.roc/Stderr.roc/Stdin.roc/Tty.roc
## `import Host` and compile here UNCHANGED. Each stdout/stderr call mints the
## process stream resource and drops it after the write (drop-balanced by B0).
import IOErr exposing [IOErr]
import Streams
import CliOut
import CliIn
import CliEnv
import CliTty
import FsOps
import Clocks
import RandomHost
import LocaleHost
import SubprocessHost
import Sockets
import HttpHost
import InternalHttp
Host :: [].{
	NativeOsStr : [Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]
	Cmd : { args : List(NativeOsStr), clear_envs : Bool, envs : List(NativeOsStr), program : NativeOsStr }
	CmdOutputSuccess : { stderr_bytes : List(U8), stdout_bytes : List(U8) }
	CmdOutputFailure : { stderr_bytes : List(U8), stdout_bytes : List(U8), exit_code : I32 }
	cmd_exec_exit_code! : Cmd => Try(I32, IOErr)
	cmd_exec_exit_code! = |c| SubprocessHost.exec_exit_code!(c)
	cmd_exec_status! : Cmd => Try(I32, IOErr)
	cmd_exec_status! = |c| SubprocessHost.exec_status!(c)
	cmd_exec_output! : Cmd => Try(CmdOutputSuccess, [NonZeroExitCode(CmdOutputFailure), FailedToGetExitCode(IOErr)])
	cmd_exec_output! = |c| SubprocessHost.exec_output!(c)
	cmd_exec_output_inherit_stdin! : Cmd => Try(CmdOutputSuccess, [NonZeroExitCode(CmdOutputFailure), FailedToGetExitCode(IOErr)])
	cmd_exec_output_inherit_stdin! = |c| SubprocessHost.exec_output_inherit_stdin!(c)
	stdout_line! : Str => Try({}, [StdoutErr(IOErr)])
	stdout_line! = |s| out_write!(CliOut.get_stdout!({}), Str.to_utf8(Str.concat(s, "\n")))
	stdout_write! : Str => Try({}, [StdoutErr(IOErr)])
	stdout_write! = |s| out_write!(CliOut.get_stdout!({}), Str.to_utf8(s))
	stdout_write_bytes! : List(U8) => Try({}, [StdoutErr(IOErr)])
	stdout_write_bytes! = |b| out_write!(CliOut.get_stdout!({}), b)

	stderr_line! : Str => Try({}, [StderrErr(IOErr)])
	stderr_line! = |s| err_write!(CliOut.get_stderr!({}), Str.to_utf8(Str.concat(s, "\n")))
	stderr_write! : Str => Try({}, [StderrErr(IOErr)])
	stderr_write! = |s| err_write!(CliOut.get_stderr!({}), Str.to_utf8(s))
	stderr_write_bytes! : List(U8) => Try({}, [StderrErr(IOErr)])
	stderr_write_bytes! = |b| err_write!(CliOut.get_stderr!({}), b)

	stdin_line! : () => Try(Str, [EndOfFile, StdinErr(IOErr)])
	stdin_line! = || CliIn.read_line!({})
	stdin_bytes! : () => Try(List(U8), [EndOfFile, StdinErr(IOErr)])
	stdin_bytes! = || {
		match Streams.read!(CliIn.get_stdin!({}), 4096) {
			Ok([]) => Err(EndOfFile)
			Ok(bytes) => Ok(bytes)
			Err(StreamErr(e)) => Err(StdinErr(e))
		}
	}
	stdin_read_to_end! : () => Try(List(U8), [StdinErr(IOErr)])
	stdin_read_to_end! = || CliIn.read_to_end!({})

	env_platform! : () => { arch : [X86, X64, ARM, AARCH64, OTHER(Str)], os : [LINUX, MACOS, WINDOWS, OTHER(Str)] }
	env_platform! = || CliEnv.platform!({})

	utc_now! : () => Try(U128, [ClockBeforeEpoch])
	utc_now! = || Clocks.wall_now!({})
	sleep_millis! : U64 => {}
	sleep_millis! = |ms| Clocks.sleep_millis!(ms)
	random_seed_u64! : () => Try(U64, [RandomErr(IOErr)])
	random_seed_u64! = || RandomHost.seed_u64!({})
	random_seed_u32! : () => Try(U32, [RandomErr(IOErr)])
	random_seed_u32! = || RandomHost.seed_u32!({})
	locale_get! : () => Try(Str, [NotAvailable])
	locale_get! = || LocaleHost.get!({})
	## List(Str) built in Roc from count/at (no host RocList<RocStr>, R-B5).
	locale_all! : () => List(Str)
	locale_all! = || collect_locales!(LocaleHost.count!({}), 0, [])

	## basic-cli's Tcp surface: the handle IS the socket resource; streams are
	## minted per call (drop-balanced). Timeouts via tcp_set_read_timeout!.
	TcpStream : Sockets.TcpSocket
	tcp_connect! : Str, U16, U64 => Try(TcpStream, Str)
	tcp_connect! = |host, port, timeout_ms| {
		match Sockets.tcp_connect!(host, port) {
			Ok(sock) => {
				Sockets.tcp_set_read_timeout!(sock, timeout_ms)
				Ok(sock)
			}
			Err(ConnectErr(e)) => Err(IOErr.to_str(e))
		}
	}
	tcp_read_up_to! : TcpStream, U64, U64 => Try(List(U8), Str)
	tcp_read_up_to! = |sock, max, timeout_ms| {
		Sockets.tcp_set_read_timeout!(sock, timeout_ms)
		match Streams.read!(Sockets.tcp_input!(sock), max) {
			Ok(b) => Ok(b)
			Err(StreamErr(e)) => Err(IOErr.to_str(e))
		}
	}
	tcp_read_exactly! : TcpStream, U64, U64 => Try(List(U8), Str)
	tcp_read_exactly! = |sock, n, timeout_ms| {
		Sockets.tcp_set_read_timeout!(sock, timeout_ms)
		read_exact_loop!(sock, n, [])
	}
	tcp_read_until! : TcpStream, U8, U64, U64 => Try(List(U8), Str)
	tcp_read_until! = |sock, delim, max, timeout_ms| {
		Sockets.tcp_set_read_timeout!(sock, timeout_ms)
		read_until_loop!(sock, delim, max, [])
	}
	tcp_write! : TcpStream, List(U8), U64 => Try({}, Str)
	tcp_write! = |sock, bytes, _timeout_ms| {
		match Streams.write!(Sockets.tcp_output!(sock), bytes) {
			Ok({}) => Ok({})
			Err(StreamErr(e)) => Err(IOErr.to_str(e))
		}
	}

	http_send_request! : InternalHttp.RequestToAndFromHost => Try(InternalHttp.ResponseToAndFromHost, InternalHttp.TransportErr)
	http_send_request! = |req| {
		# Pass the body stream through unchanged (H5); Http collects on demand
		# via read_body_to_end!, so the shim no longer eagerly reads the body.
		match HttpHost.send!(req) {
			Ok(resp) => Ok({ status: resp.status, headers: split_headers(resp.headers_flat), body_stream: resp.body_stream })
			Err(e) => Err(e)
		}
	}

	tty_enable_raw_mode! : () => {}
	tty_enable_raw_mode! = || CliTty.enable_raw_mode!({})
	tty_disable_raw_mode! : () => {}
	tty_disable_raw_mode! = || CliTty.disable_raw_mode!({})
	# ---- basic-cli's file/dir/env seam (P15): its Path.roc/File.roc/Env.roc
	# compile UNCHANGED over these, which are pure Roc over FsOps' bytes
	# primitives (P11) and CliEnv. NativePath is the same union as OsStr.
	NativePath : [Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]
	PathType : [File, Dir, SymLink, Other]
	## basic-cli's buffered reader handle IS a sync-io input stream.
	FileReader : Streams.InputStream

	## NativePath -> FsOps bytes. Windows UTF-16 units are not representable on
	## this host and surface as an IOErr rather than a silent transcoding.
	native_bytes : NativePath -> Try(List(U8), IOErr)
	native_bytes = |p| match p {
		Utf8(s) => Ok(Str.to_utf8(s))
		UnixBytes(b) => Ok(b)
		WindowsU16s(_) => Err(Other("windows paths are not supported on this host"))
	}
	bytes_native : List(U8) -> NativePath
	bytes_native = |b| match Str.from_utf8(b) {
		Ok(s) => Utf8(s)
		Err(_) => UnixBytes(b)
	}
	file_bytes : NativePath -> Try(List(U8), [FileErr(IOErr)])
	file_bytes = |p| native_bytes(p).map_err(|e| FileErr(e))
	dir_bytes : NativePath -> Try(List(U8), [DirErr(IOErr)])
	dir_bytes = |p| native_bytes(p).map_err(|e| DirErr(e))

	dir_create! : NativePath => Try({}, [DirErr(IOErr)])
	dir_create! = |p| FsOps.create_dir!(dir_bytes(p)?)
	dir_create_all! : NativePath => Try({}, [DirErr(IOErr)])
	dir_create_all! = |p| FsOps.create_all!(dir_bytes(p)?)
	dir_delete_empty! : NativePath => Try({}, [DirErr(IOErr)])
	dir_delete_empty! = |p| FsOps.delete_empty!(dir_bytes(p)?)
	dir_delete_all! : NativePath => Try({}, [DirErr(IOErr)])
	dir_delete_all! = |p| FsOps.delete_all!(dir_bytes(p)?)
	## basic-cli lists entries joined to the directory (`dir/name`), as
	## `std::fs::read_dir`'s `entry.path()` does; FsOps yields bare names.
	dir_list! : NativePath => Try(List(NativePath), [DirErr(IOErr)])
	dir_list! = |p| {
		dir = dir_bytes(p)?
		Ok(List.map(FsOps.list!(dir)?, |name| bytes_native(join_entry(dir, name))))
	}
	join_entry : List(U8), List(U8) -> List(U8)
	join_entry = |dir, name| match List.last(dir) {
		Ok(c) => if c == '/' { List.concat(dir, name) } else { List.concat(List.append(dir, '/'), name) }
		Err(_) => name
	}

	file_read_bytes! : NativePath => Try(List(U8), [FileErr(IOErr)])
	file_read_bytes! = |p| FsOps.read!(file_bytes(p)?)
	file_write_bytes! : NativePath, List(U8) => Try({}, [FileErr(IOErr)])
	file_write_bytes! = |p, bytes| FsOps.write!(file_bytes(p)?, bytes)
	file_read_utf8! : NativePath => Try(Str, [FileErr(IOErr)])
	file_read_utf8! = |p| match Str.from_utf8(FsOps.read!(file_bytes(p)?)?) {
		Ok(s) => Ok(s)
		Err(_) => Err(FileErr(Other("file is not valid UTF-8")))
	}
	file_write_utf8! : NativePath, Str => Try({}, [FileErr(IOErr)])
	file_write_utf8! = |p, s| FsOps.write!(file_bytes(p)?, Str.to_utf8(s))
	file_open_reader! : NativePath, U64 => Try(FileReader, [FileErr(IOErr)])
	file_open_reader! = |p, _capacity| FsOps.open_read!(file_bytes(p)?)
	file_read_line! : FileReader => Try(List(U8), [FileErr(IOErr)])
	file_read_line! = |r| Streams.read_until!(r, 10, 1_048_576).map_err(|StreamErr(e)| FileErr(e))
	file_delete! : NativePath => Try({}, [FileErr(IOErr)])
	file_delete! = |p| FsOps.delete!(file_bytes(p)?)
	file_size_in_bytes! : NativePath => Try(U64, [FileErr(IOErr)])
	file_size_in_bytes! = |p| FsOps.size!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_is_executable! : NativePath => Try(Bool, [FileErr(IOErr)])
	file_is_executable! = |p| FsOps.executable!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_is_readable! : NativePath => Try(Bool, [FileErr(IOErr)])
	file_is_readable! = |p| FsOps.readable!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_is_writable! : NativePath => Try(Bool, [FileErr(IOErr)])
	file_is_writable! = |p| FsOps.writable!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_time_accessed! : NativePath => Try(U128, [FileErr(IOErr)])
	file_time_accessed! = |p| FsOps.accessed!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_time_modified! : NativePath => Try(U128, [FileErr(IOErr)])
	file_time_modified! = |p| FsOps.modified!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_time_created! : NativePath => Try(U128, [FileErr(IOErr)])
	file_time_created! = |p| FsOps.created!(file_bytes(p)?).map_err(|e| FileErr(e))
	file_hard_link! : NativePath, NativePath => Try({}, [FileErr(IOErr)])
	file_hard_link! = |from, to| FsOps.hard_link!(file_bytes(from)?, file_bytes(to)?)
	file_rename! : NativePath, NativePath => Try({}, [FileErr(IOErr)])
	file_rename! = |from, to| FsOps.rename!(file_bytes(from)?, file_bytes(to)?)
	path_type! : NativePath => Try(PathType, IOErr)
	path_type! = |p| FsOps.kind!(native_bytes(p)?)

	env_var! : NativeOsStr => Try(NativeOsStr, [VarNotFound(NativeOsStr), EnvErr(IOErr)])
	env_var! = |name| match name {
		Utf8(s) => match CliEnv.var!(s) {
			Ok(v) => Ok(Utf8(v))
			Err(VarNotFound(n)) => Err(VarNotFound(Utf8(n)))
			Err(EnvErr(e)) => Err(EnvErr(e))
		}
		other => Err(VarNotFound(other))
	}
	env_dict! : () => List((NativeOsStr, NativeOsStr))
	env_dict! = || env_dict_from!(0, CliEnv.env_count!({}), [])
	env_dict_from! : U64, U64, List((NativeOsStr, NativeOsStr)) => List((NativeOsStr, NativeOsStr))
	env_dict_from! = |i, n, acc| if i >= n { acc } else {
		entry = CliEnv.env_at!(i)
		env_dict_from!(i + 1, n, List.append(acc, (Utf8(entry.name), Utf8(entry.value))))
	}
	env_cwd! : () => Try(NativePath, [CwdUnavailable])
	env_cwd! = || Ok(Utf8(FsOps.cwd!({})))
	env_set_cwd! : NativePath => Try({}, IOErr)
	env_set_cwd! = |p| match p {
		Utf8(s) => {
			FsOps.set_cwd!(s)
			Ok({})
		}
		UnixBytes(b) => match Str.from_utf8(b) {
			Ok(s) => {
				FsOps.set_cwd!(s)
				Ok({})
			}
			Err(_) => Err(Other("cwd is not valid UTF-8"))
		}
		WindowsU16s(_) => Err(Other("windows paths are not supported on this host"))
	}
	env_exe_path! : () => Try(NativePath, [ExePathUnavailable])
	env_exe_path! = || Ok(Utf8(CliEnv.exe_path!({})))
	env_temp_dir! : () => NativePath
	env_temp_dir! = || Utf8(CliEnv.temp_dir!({}))
}

out_write! : Streams.OutputStream, List(U8) => Try({}, [StdoutErr(IOErr)])
out_write! = |stream, bytes| {
	match Streams.write!(stream, bytes) {
		Ok({}) => Ok({})
		Err(StreamErr(e)) => Err(StdoutErr(e))
	}
}
err_write! : Streams.OutputStream, List(U8) => Try({}, [StderrErr(IOErr)])
err_write! = |stream, bytes| {
	match Streams.write!(stream, bytes) {
		Ok({}) => Ok({})
		Err(StreamErr(e)) => Err(StderrErr(e))
	}
}

collect_locales! : U64, U64, List(Str) => List(Str)
collect_locales! = |n, i, acc| if i >= n { acc } else { collect_locales!(n, i + 1, List.append(acc, LocaleHost.at!(i))) }

read_exact_loop! : Sockets.TcpSocket, U64, List(U8) => Try(List(U8), Str)
read_exact_loop! = |sock, n, acc| {
	if List.len(acc) >= n {
		Ok(acc)
	} else {
		match Streams.read!(Sockets.tcp_input!(sock), n - List.len(acc)) {
			Ok([]) => Err("unexpected end of stream")
			Ok(chunk) => read_exact_loop!(sock, n, List.concat(acc, chunk))
			Err(StreamErr(e)) => Err(IOErr.to_str(e))
		}
	}
}
read_until_loop! : Sockets.TcpSocket, U8, U64, List(U8) => Try(List(U8), Str)
read_until_loop! = |sock, delim, max, acc| {
	if List.len(acc) >= max {
		Ok(acc)
	} else {
		match Streams.read!(Sockets.tcp_input!(sock), 1) {
			Ok([]) => Ok(acc)
			Ok([b]) => if b == delim { Ok(List.append(acc, b)) } else { read_until_loop!(sock, delim, max, List.append(acc, b)) }
			Ok(other) => read_until_loop!(sock, delim, max, List.concat(acc, other))
			Err(StreamErr(e)) => Err(IOErr.to_str(e))
		}
	}
}

## name\0value\0... -> [(name, value), ...]
split_headers : List(U8) -> List((Str, Str))
split_headers = |flat| {
	parts = split_on_nul(flat, [], [])
	pair_up(parts, [])
}
split_on_nul : List(U8), List(U8), List(Str) -> List(Str)
split_on_nul = |bytes, cur, acc| {
	match bytes {
		[] => if List.is_empty(cur) and List.is_empty(acc) { [] } else { List.append(acc, Str.from_utf8_lossy(cur)) }
		[0, .. as rest] => split_on_nul(rest, [], List.append(acc, Str.from_utf8_lossy(cur)))
		[b, .. as rest] => split_on_nul(rest, List.append(cur, b), acc)
	}
}
pair_up : List(Str), List((Str, Str)) -> List((Str, Str))
pair_up = |parts, acc| {
	match parts {
		[k, v, .. as rest] => pair_up(rest, List.append(acc, (k, v)))
		_ => acc
	}

}

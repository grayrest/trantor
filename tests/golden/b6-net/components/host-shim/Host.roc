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

	env_var! : Str => Try(Str, [VarNotFound(Str), EnvErr(IOErr)])
	env_var! = |name| CliEnv.var!(name)
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
		match HttpHost.send!(req) {
			# The primitive now streams the body (H5); the eager basic-cli-shaped
			# response wants the whole thing, so collect it here. (The streaming
			# app-facing Response is HC3; this keeps Http.get_utf8!/get! working.)
			Ok(resp) => Ok({ status: resp.status, headers: split_headers(resp.headers_flat), body: collect_stream!(resp.body_stream, []) })
			Err(e) => Err(e)
		}
	}

	tty_enable_raw_mode! : () => {}
	tty_enable_raw_mode! = || CliTty.enable_raw_mode!({})
	tty_disable_raw_mode! : () => {}
	tty_disable_raw_mode! = || CliTty.disable_raw_mode!({})
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

## Drain an InputStream to end. `read!` owns its handle each call (released via
## the resource contract), so `stream` is threaded through the recursion: Roc
## re-incs it before each `read!`, and drops it at the base case — drop-balanced.
## A mid-stream StreamErr ends the collect with what was read (H15).
collect_stream! : Streams.InputStream, List(U8) => List(U8)
collect_stream! = |stream, acc| {
	match Streams.read!(stream, 65536) {
		Ok(chunk) => if List.is_empty(chunk) { acc } else { collect_stream!(stream, List.concat(acc, chunk)) }
		Err(_) => acc
	}
}

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

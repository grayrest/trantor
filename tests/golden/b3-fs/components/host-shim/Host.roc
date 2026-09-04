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
Host :: [].{
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

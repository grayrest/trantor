platform ""
	requires {
		main! : List([Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]) => Try({}, [Exit(I32), ..])
	}
	exposes [Cmd, Env, File, Http, IOErr, Locale, OsStr, Path, Random, Sleep, Stdin, Stdout, Stderr, Tcp, Tty, Url, Utc, Udp, Sockets, Streams, Temporal, StrPath, OsPath, TempTest]
	packages {
		http: "https://github.com/roc-lang/http/releases/download/1.0.0/6ZUwqYhCS8PU9Mo6MF7oV82ET2o7KYb57CLKDq4cq4sS.tar.zst",
	}
	provides { "roc_main": main_for_host! }
	hosted {
		"hematite__cell__put": Cell.put!,
		"hematite__cell__get": Cell.get!,
		"hematite__cli_host__arg_count": CliEnv.arg_count!,
		"hematite__cli_host__arg_at": CliEnv.arg_at!,
		"hematite__cli_host__var": CliEnv.var!,
		"hematite__cli_host__env_count": CliEnv.env_count!,
		"hematite__cli_host__env_at": CliEnv.env_at!,
		"hematite__cli_host__platform": CliEnv.platform!,
		"hematite__cli_host__cwd": CliEnv.cwd!,
		"hematite__cli_host__exe_path": CliEnv.exe_path!,
		"hematite__cli_host__temp_dir": CliEnv.temp_dir!,
		"hematite__cli_host__get_stdin": CliIn.get_stdin!,
		"hematite__cli_host__read_line": CliIn.read_line!,
		"hematite__cli_host__read_to_end": CliIn.read_to_end!,
		"hematite__cli_host__get_stdout": CliOut.get_stdout!,
		"hematite__cli_host__get_stderr": CliOut.get_stderr!,
		"hematite__cli_host__enable_raw_mode": CliTty.enable_raw_mode!,
		"hematite__cli_host__disable_raw_mode": CliTty.disable_raw_mode!,
		"hematite__clocks_host__wall_now": Clocks.wall_now!,
		"hematite__clocks_host__monotonic_now": Clocks.monotonic_now!,
		"hematite__clocks_host__sleep_millis": Clocks.sleep_millis!,
		"hematite__fs_unconfined__preopen_count": Fs.preopen_count!,
		"hematite__fs_unconfined__preopen_at": Fs.preopen_at!,
		"hematite__fs_unconfined__open_at": Fs.open_at!,
		"hematite__fs_unconfined__read_via_stream": Fs.read_via_stream!,
		"hematite__fs_unconfined__read_file_at": Fs.read_file_at!,
		"hematite__fs_unconfined__write_file_at": Fs.write_file_at!,
		"hematite__fs_unconfined__stat_at": Fs.stat_at!,
		"hematite__fs_unconfined__read_dir_at": Fs.read_dir_at!,
		"hematite__fs_unconfined__create_dir_at": Fs.create_dir_at!,
		"hematite__fs_unconfined__create_dir_all_at": Fs.create_dir_all_at!,
		"hematite__fs_unconfined__remove_dir_at": Fs.remove_dir_at!,
		"hematite__fs_unconfined__remove_dir_all_at": Fs.remove_dir_all_at!,
		"hematite__fs_unconfined__unlink_at": Fs.unlink_at!,
		"hematite__fs_unconfined__rename_at": Fs.rename_at!,
		"hematite__fs_unconfined__link_at": Fs.link_at!,
		"hematite__fs_unconfined__live": Fs.live!,
		"hematite__locale_host__get": LocaleHost.get!,
		"hematite__locale_host__count": LocaleHost.count!,
		"hematite__locale_host__at": LocaleHost.at!,
		"hematite__random_host__seed_u64": RandomHost.seed_u64!,
		"hematite__random_host__seed_u32": RandomHost.seed_u32!,
		"hematite__subprocess_host__exec_exit_code": SubprocessHost.exec_exit_code!,
		"hematite__subprocess_host__exec_status": SubprocessHost.exec_status!,
		"hematite__subprocess_host__exec_output": SubprocessHost.exec_output!,
		"hematite__subprocess_host__exec_output_inherit_stdin": SubprocessHost.exec_output_inherit_stdin!,
		"hematite__http_host__send": HttpHost.send!,
		"hematite__sync_io__read": Streams.read!,
		"hematite__sync_io__write": Streams.write!,
		"hematite__sync_io__read_until": Streams.read_until!,
		"hematite__sockets_host__resolve": Sockets.resolve!,
		"hematite__sockets_host__tcp_connect": Sockets.tcp_connect!,
		"hematite__sockets_host__tcp_listen": Sockets.tcp_listen!,
		"hematite__sockets_host__tcp_accept": Sockets.tcp_accept!,
		"hematite__sockets_host__tcp_input": Sockets.tcp_input!,
		"hematite__sockets_host__tcp_output": Sockets.tcp_output!,
		"hematite__sockets_host__tcp_set_read_timeout": Sockets.tcp_set_read_timeout!,
		"hematite__sockets_host__tcp_local_port": Sockets.tcp_local_port!,
		"hematite__sockets_host__udp_bind": Sockets.udp_bind!,
		"hematite__sockets_host__udp_send_to": Sockets.udp_send_to!,
		"hematite__sockets_host__udp_recv": Sockets.udp_recv!,
		"hematite__sockets_host__udp_local_port": Sockets.udp_local_port!,
		"hematite__sockets_host__live": Sockets.live!,
		"hematite__temporal_host__calendar_from_id": Temporal.calendar_from_id!,
		"hematite__temporal_host__calendar_id": Temporal.calendar_id!,
		"hematite__temporal_host__time_zone_from_id": Temporal.time_zone_from_id!,
		"hematite__temporal_host__time_zone_id": Temporal.time_zone_id!,
		"hematite__temporal_host__date_add": Temporal.date_add!,
		"hematite__temporal_host__date_until": Temporal.date_until!,
		"hematite__temporal_host__date_day_of_week": Temporal.date_day_of_week!,
		"hematite__temporal_host__zdt_from_epoch_ns": Temporal.zdt_from_epoch_ns!,
		"hematite__temporal_host__zdt_epoch_ns": Temporal.zdt_epoch_ns!,
		"hematite__temporal_host__zdt_with_time_zone": Temporal.zdt_with_time_zone!,
		"hematite__temporal_host__zdt_plain_date": Temporal.zdt_plain_date!,
		"hematite__temporal_host__zdt_plain_time": Temporal.zdt_plain_time!,
		"hematite__temporal_host__zdt_offset_seconds": Temporal.zdt_offset_seconds!,
		"hematite__temporal_host__zdt_to_str": Temporal.zdt_to_str!,
		"hematite__temporal_host__live": Temporal.live!,
		"hematite__testnet_host__start_test_server": TestNet.start_test_server!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libmain_driver.a", "libcell.a", "libclocks_host.a", "libhttp_host.a", "liblocale_host.a", "librandom_host.a", "libsubprocess_host.a", "libsync_io.a", "libtemporal_host.a", "libtestnet_host.a", "libcli_host.a", "libfs_unconfined.a", "libsockets_host.a", app] },
		x64mac: { inputs: ["libmain_driver.a", "libcell.a", "libclocks_host.a", "libhttp_host.a", "liblocale_host.a", "librandom_host.a", "libsubprocess_host.a", "libsync_io.a", "libtemporal_host.a", "libtestnet_host.a", "libcli_host.a", "libfs_unconfined.a", "libsockets_host.a", app] },
	}

import Cell
import CliEnv
import CliIn
import CliOut
import CliTty
import Clocks
import Fs
import LocaleHost
import RandomHost
import SubprocessHost
import HttpHost
import Streams
import Sockets
import Temporal
import TestNet
import Cmd
import Env
import File
import Http
import IOErr
import Locale
import OsStr
import Path
import Random
import Sleep
import Stdin
import Stdout
import Stderr
import Tcp
import Tty
import Url
import Utc
import Udp
import StrPath
import OsPath
import TempTest

main_for_host! : () => I32
main_for_host! = || {
	match main!(host_args!(0, CliEnv.arg_count!({}), [])) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

## basic-cli's contract: argv as OS strings (this host mints Utf8; a non-UTF-8
## argv would arrive as UnixBytes from a host that reads raw argv).
host_args! : U64, U64, List([Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))]) => List([Utf8(Str), UnixBytes(List(U8)), WindowsU16s(List(U16))])
host_args! = |i, n, acc| if i >= n { acc } else { host_args!(i + 1, n, List.append(acc, Utf8(CliEnv.arg_at!(i)))) }

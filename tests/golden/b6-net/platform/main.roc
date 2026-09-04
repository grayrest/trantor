platform ""
	requires {
		main! : List(Str) => Try({}, [Exit(I32), ..])
	}
	exposes [Stdout, Stderr, Stdin, Tty, Env, Path, Utc, Sleep, Random, Locale, Url, Cmd, OsStr, Tcp, Http, Udp, Sockets, Streams, TempTest]
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
		"hematite__testnet_host__start_tcp_echo": TestNet.start_tcp_echo!,
		"hematite__testnet_host__start_udp_echo": TestNet.start_udp_echo!,
		"hematite__testnet_host__start_httpd": TestNet.start_httpd!,
		"hematite__testnet_host__connect_and_send_later": TestNet.connect_and_send_later!,
		"hematite__testnet_host__start_https_server": TestNet.start_https_server!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libcell.a", "libclocks_host.a", "libhttp_host.a", "liblocale_host.a", "librandom_host.a", "libsubprocess_host.a", "libsync_io.a", "libtestnet_host.a", "libcli_host.a", "libfs_unconfined.a", "libsockets_host.a", "libmain_driver.a", app] },
		x64mac: { inputs: ["libcell.a", "libclocks_host.a", "libhttp_host.a", "liblocale_host.a", "librandom_host.a", "libsubprocess_host.a", "libsync_io.a", "libtestnet_host.a", "libcli_host.a", "libfs_unconfined.a", "libsockets_host.a", "libmain_driver.a", app] },
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
import TestNet
import Stdout
import Stderr
import Stdin
import Tty
import Env
import Path
import Utc
import Sleep
import Random
import Locale
import Url
import Cmd
import OsStr
import Tcp
import Http
import Udp
import TempTest
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!(Env.args!({})) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

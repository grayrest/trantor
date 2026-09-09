platform ""
	requires {
		main! : List(Str) => Try({}, [Exit(I32), ..])
	}
	exposes [Stdout, Stderr, Stdin, Tty, Env, Path, Utc, Sleep, Random, Locale, Url]
	packages {}
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
		"hematite__sync_io__read": Streams.read!,
		"hematite__sync_io__write": Streams.write!,
	}
	targets: {
		inputs_dir: "targets/",
		arm64mac: { inputs: ["libmain_driver.a", "libcell.a", "libclocks_host.a", "liblocale_host.a", "librandom_host.a", "libsync_io.a", "libcli_host.a", "libfs_unconfined.a", app] },
		x64mac: { inputs: ["libmain_driver.a", "libcell.a", "libclocks_host.a", "liblocale_host.a", "librandom_host.a", "libsync_io.a", "libcli_host.a", "libfs_unconfined.a", app] },
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
import Streams
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
import IOErr

main_for_host! : () => I32
main_for_host! = || {
	match main!(Env.args!({})) {
		Ok({}) => 0
		Err(Exit(code)) => code
		Err(_) => 1
	}
}

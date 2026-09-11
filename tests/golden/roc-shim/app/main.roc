app [main!] { pf: platform "../target/trantor/roc-shim/platform/main.roc" }

import pf.Path
import pf.Stdio
import pf.Env

## Reads the file named by the first CLI arg (via Env) and prints its line count.
main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	path = Env.path_arg!({})
	n = Path.line_count!(path) ?? 0
	Stdio.line!("${u64_to_str(n)} lines in ${path}") ?? {}
	Ok({})
}

u64_to_str : U64 -> Str
u64_to_str = |n| if n == 0 { "0" } else { go(n, "") }

go : U64, Str -> Str
go = |n, acc| {
	if n == 0 {
		acc
	} else {
		go(n // 10, Str.concat(digit_char(n % 10), acc))
	}
}

digit_char : U64 -> Str
digit_char = |d| {
	match d {
		0 => "0"
		1 => "1"
		2 => "2"
		3 => "3"
		4 => "4"
		5 => "5"
		6 => "6"
		7 => "7"
		8 => "8"
		_ => "9"
	}
}

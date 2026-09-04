app [main!] { pf: platform "../platform/main.roc" }

import pf.Mark

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Mark.ping!("hc0")
	Ok({})
}

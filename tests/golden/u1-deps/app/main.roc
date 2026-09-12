app [main!] { pf: platform "../target/trantor/u1-deps/platform/main.roc" }

import pf.Text

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	loud = Text.shout!("a-string-well-over-twenty-three-bytes-so-it-heap-allocates")
	n = Text.count!(loud)
	Text.emit!(n)
	Ok({})
}

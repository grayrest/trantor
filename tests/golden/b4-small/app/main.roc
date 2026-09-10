app [main!] { pf: platform "../target/hematite/b4-small/platform/main.roc" }
import pf.Stdout
import pf.Utc
import pf.Sleep
import pf.Random
import pf.Locale
import pf.Url
import pf.Env

main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	now = Utc.now!()
	iso = Utc.to_iso_8601(now)
	Stdout.line!(Str.concat("utc-year-prefix: ", if Str.starts_with(iso, "20") { "ok" } else { iso })) ?? {}
	Sleep.millis!(1)
	Stdout.line!("slept: ok") ?? {}
	r = match Random.seed_u64!() {
		Ok(_) => "ok"
		Err(_) => "err"
	}
	Stdout.line!(Str.concat("random: ", r)) ?? {}
	loc = match Locale.get!() {
		Ok(l) => if Str.is_empty(Locale.to_str(l)) { "empty" } else { "ok" }
		Err(_) => "unavailable"
	}
	Stdout.line!(Str.concat("locale: ", loc)) ?? {}
	n_all = List.len(Locale.all!())
	Stdout.line!(Str.concat("locales-listed: ", if n_all > 0 { "some" } else { "none" })) ?? {}
	parsed = match Locale.parse(Env.var!("B4_TAG") ?? "zh-Hant-TW") {
		Ok(l) => Locale.to_str(l)
		Err(_) => "parse-failed"
	}
	Stdout.line!(Str.concat("locale-parse: ", parsed)) ?? {}
	u = Url.parse("https://example.com:8080/a/b?x=1#frag")
	host = match u {
		Ok(url) => Url.host(url)
		Err(_) => "url-parse-failed"
	}
	Stdout.line!(Str.concat("url-host: ", host)) ?? {}
	Ok({})
}

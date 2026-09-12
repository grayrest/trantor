app [main!] { pf: platform "../target/trantor/b7-temporal/platform/main.roc" }
import pf.Stdout
import pf.Temporal
import pf.Gauge

## B7 consumes the trantor-temporal package from a world that is NOT
## trantor-cli. The behaviour checks are deliberately thin — the package's own
## verify.sh pins sixty of them — and what matters here is that the package
## composes at all against a different driver, and that every resource it hands
## out is dropped (exit code = live handles; 0 = balanced).
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	ny = Temporal.time_zone_from_id!("America/New_York") ?? return(Err(Exit(3)))
	tokyo = Temporal.time_zone_from_id!("Asia/Tokyo") ?? return(Err(Exit(4)))

	one_month = { months: 1 }
	jan31 = Temporal.plain_date({ year: 2024, month: 1, day: 31 })
	added = jan31.add!(one_month) ?? return(Err(Exit(5)))
	Stdout.line!(Str.concat("date-add: ", added.to_str())) ?? {}

	# half an hour before the 2024-03-10 US jump, so the offset is standard -05:00
	at : Temporal.PlainTime
	at = { hour: 1, minute: 30 }
	zdt = Temporal.zoned!(Temporal.plain_date({ year: 2024, month: 3, day: 10 }), at, ny) ?? return(Err(Exit(6)))
	ny_str = zdt.to_str!() ?? return(Err(Exit(7)))
	Stdout.line!(Str.concat("zdt-ny: ", ny_str)) ?? {}

	in_tokyo = zdt.with_time_zone!(tokyo) ?? return(Err(Exit(8)))
	tokyo_str = in_tokyo.to_str!() ?? return(Err(Exit(9)))
	Stdout.line!(Str.concat("zdt-tokyo: ", tokyo_str)) ?? {}
	same = if zdt.epoch_ns!() == in_tokyo.epoch_ns!() { "yes" } else { "no" }
	Stdout.line!(Str.concat("same-instant: ", same)) ?? {}

	# the package's own module, reached from a world with no basic-cli shim
	Stdout.line!(Str.concat("day-of-week: ", U8.to_str(Temporal.plain_date({ year: 2024, month: 2, day: 29 }).iso_day_of_week()))) ?? {}

	live = Gauge.live!({})
	if live == 0 { Ok({}) } else { Err(Exit(10 + live)) }
}

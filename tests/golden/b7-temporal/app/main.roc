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
	iso = Temporal.iso!({}) ?? return(Err(Exit(2)))
	ny = Temporal.time_zone_from_id!("America/New_York") ?? return(Err(Exit(3)))
	tokyo = Temporal.time_zone_from_id!("Asia/Tokyo") ?? return(Err(Exit(4)))

	one_month = { years: 0, months: 1, weeks: 0, days: 0, hours: 0, minutes: 0, seconds: 0, milliseconds: 0, microseconds: 0, nanoseconds: 0 }
	added = Temporal.add!({ year: 2024, month: 1, day: 31 }, one_month, iso) ?? return(Err(Exit(5)))
	Stdout.line!(Str.concat("date-add: ", Temporal.date_to_str(added))) ?? {}

	# half an hour before the 2024-03-10 US jump, so the offset is standard -05:00
	at = { hour: 1, minute: 30, second: 0, millisecond: 0, microsecond: 0, nanosecond: 0 }
	zdt = Temporal.zdt_from_wall_clock!({ year: 2024, month: 3, day: 10 }, at, ny, iso) ?? return(Err(Exit(6)))
	ny_str = Temporal.zdt_to_str!(zdt) ?? return(Err(Exit(7)))
	Stdout.line!(Str.concat("zdt-ny: ", ny_str)) ?? {}

	in_tokyo = Temporal.zdt_with_time_zone!(zdt, tokyo) ?? return(Err(Exit(8)))
	tokyo_str = Temporal.zdt_to_str!(in_tokyo) ?? return(Err(Exit(9)))
	Stdout.line!(Str.concat("zdt-tokyo: ", tokyo_str)) ?? {}
	same = if Temporal.zdt_epoch_ns!(zdt) == Temporal.zdt_epoch_ns!(in_tokyo) { "yes" } else { "no" }
	Stdout.line!(Str.concat("same-instant: ", same)) ?? {}

	# the package's own module, reached from a world with no basic-cli shim
	Stdout.line!(Str.concat("day-of-week: ", U8.to_str(Temporal.iso_day_of_week({ year: 2024, month: 2, day: 29 })))) ?? {}

	live = Gauge.live!({})
	if live == 0 { Ok({}) } else { Err(Exit(10 + live)) }
}

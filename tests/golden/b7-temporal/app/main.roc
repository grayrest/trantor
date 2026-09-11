app [main!] { pf: platform "../target/trantor/b7-temporal/platform/main.roc" }
import pf.Stdout
import pf.Temporal

## Exercises roc:temporal: calendar arithmetic on plain records, a zoned
## instant in two zones (across the 2024-03-10 US DST switch), and the
## resource drop balance (exit code = live handles; 0 = balanced).
main! : List(Str) => Try({}, [Exit(I32), ..])
main! = |_args| {
	iso = Temporal.calendar_from_id!("iso8601") ?? return(Err(Exit(2)))
	one_month = { years: 0, months: 1, weeks: 0, days: 0, hours: 0, minutes: 0, seconds: 0, milliseconds: 0, microseconds: 0, nanoseconds: 0 }
	added = Temporal.date_add!({ year: 2024, month: 1, day: 31 }, one_month, iso) ?? return(Err(Exit(3)))
	Stdout.line!(Str.concat("date-add: ", fmt_date(added))) ?? {}
	until = Temporal.date_until!({ year: 2024, month: 1, day: 1 }, { year: 2024, month: 12, day: 25 }, iso) ?? return(Err(Exit(4)))
	Stdout.line!(Str.concat("date-until-days: ", I64.to_str(until.days))) ?? {}
	dow = Temporal.date_day_of_week!({ year: 2024, month: 2, day: 29 }, iso) ?? return(Err(Exit(5)))
	Stdout.line!(Str.concat("day-of-week: ", U8.to_str(dow))) ?? {}

	ny = Temporal.time_zone_from_id!("America/New_York") ?? return(Err(Exit(6)))
	tokyo = Temporal.time_zone_from_id!("Asia/Tokyo") ?? return(Err(Exit(7)))
	# 2024-03-10T06:30:00Z — 01:30 in New York, half an hour before the DST jump.
	zdt = Temporal.zdt_from_epoch_ns!(1_710_052_200_000_000_000, ny) ?? return(Err(Exit(8)))
	Stdout.line!(Str.concat("zdt-ny: ", Temporal.zdt_to_str!(zdt))) ?? {}
	Stdout.line!(Str.concat("zdt-ny-hour: ", U8.to_str(Temporal.zdt_plain_time!(zdt).hour))) ?? {}
	Stdout.line!(Str.concat("zdt-ny-offset-s: ", I64.to_str(Temporal.zdt_offset_seconds!(zdt)))) ?? {}
	in_tokyo = Temporal.zdt_with_time_zone!(zdt, tokyo) ?? return(Err(Exit(9)))
	Stdout.line!(Str.concat("zdt-tokyo: ", Temporal.zdt_to_str!(in_tokyo))) ?? {}
	Stdout.line!(Str.concat("zdt-tokyo-date: ", fmt_date(Temporal.zdt_plain_date!(in_tokyo)))) ?? {}
	same_instant = Temporal.zdt_epoch_ns!(in_tokyo) == 1_710_052_200_000_000_000
	Stdout.line!(Str.concat("same-instant: ", if same_instant { "yes" } else { "no" })) ?? {}
	Stdout.line!(Str.concat("tz-id: ", Temporal.time_zone_id!(tokyo))) ?? {}

	# Everything above is dropped by here except what Roc still holds; report the balance.
	live = Temporal.live!({})
	if live == 0 { Ok({}) } else { Err(Exit(live)) }
}

fmt_date : Temporal.PlainDate -> Str
fmt_date = |d| Str.join_with([I32.to_str(d.year), pad2(d.month), pad2(d.day)], "-")

pad2 : U8 -> Str
pad2 = |n| if n < 10 { Str.concat("0", U8.to_str(n)) } else { U8.to_str(n) }

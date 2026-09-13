app [main!] { pf: platform "../target/trantor/b7-temporal/platform/main.roc" }

import pf.OsStr exposing [OsStr]
import pf.Stdout
import pf.Temporal

## Behaviour checks are deliberately thin: the package's own verify.sh pins
## dozens of them, and duplicating them is how the old copy drifted. What
## matters here is that the package composes, and — measured by the verify.sh
## alloc gauge, not by an exit code — that every resource it hands out is freed.
main! : List(OsStr) => Try({}, _)
main! = |_args| {
	ny = Temporal.time_zone_from_id!("America/New_York") ? |_| NyZone
	tokyo = Temporal.time_zone_from_id!("Asia/Tokyo") ? |_| TokyoZone

	jan31 = Temporal.plain_date({ year: 2024, month: 1, day: 31 })
	added = jan31.add!({ months: 1 }) ? |_| DateAdd
	Stdout.line!("date-add: ${added.to_str()}")?

	# half an hour before the 2024-03-10 US jump, so the offset is standard -05:00
	at : Temporal.PlainTime
	at = { hour: 1, minute: 30 }
	zdt = Temporal.zoned!(Temporal.plain_date({ year: 2024, month: 3, day: 10 }), at, ny) ? |_| Zoned
	ny_str = zdt.to_str!() ? |_| NyStr
	Stdout.line!("zdt-ny: ${ny_str}")?

	in_tokyo = zdt.with_time_zone!(tokyo) ? |_| ToTokyo
	tokyo_str = in_tokyo.to_str!() ? |_| TokyoStr
	Stdout.line!("zdt-tokyo: ${tokyo_str}")?
	same = if zdt.epoch_ns!() == in_tokyo.epoch_ns!() { "yes" } else { "no" }
	Stdout.line!("same-instant: ${same}")?

	Stdout.line!("day-of-week: ${Temporal.plain_date({ year: 2024, month: 2, day: 29 }).day_of_week().to_str()}")
}

## roc:temporal — TC39 Temporal-shaped calendar and time-zone arithmetic,
## host-backed by temporal_rs (P13). PlainDate/PlainTime/Duration are plain
## records (ISO fields); ZonedDateTime/TimeZone/Calendar are refcounted host
## resources (P5). Overflow is Temporal's default `constrain`.
Temporal :: [].{
	ZonedDateTime :: Box(U64)
	TimeZone :: Box(U64)
	Calendar :: Box(U64)

	PlainDate : { year : I32, month : U8, day : U8 }
	PlainTime : { hour : U8, minute : U8, second : U8, millisecond : U16, microsecond : U16, nanosecond : U16 }
	Duration : {
		years : I64, months : I64, weeks : I64, days : I64,
		hours : I64, minutes : I64, seconds : I64, milliseconds : I64,
		microseconds : I64, nanoseconds : I64,
	}
	## Temporal's error kinds: RangeError (out of range / invalid), TypeError,
	## SyntaxError (unparseable identifier), plus a catch-all.
	Err : [Range(Str), TypeErr(Str), Syntax(Str), Other(Str)]

	calendar_from_id! : Str => Try(Calendar, Err)
	calendar_id! : Calendar => Str
	time_zone_from_id! : Str => Try(TimeZone, Err)
	time_zone_id! : TimeZone => Str

	date_add! : PlainDate, Duration, Calendar => Try(PlainDate, Err)
	date_until! : PlainDate, PlainDate, Calendar => Try(Duration, Err)
	## ISO day of week, Monday = 1 … Sunday = 7.
	date_day_of_week! : PlainDate, Calendar => Try(U8, Err)

	zdt_from_epoch_ns! : I128, TimeZone => Try(ZonedDateTime, Err)
	zdt_epoch_ns! : ZonedDateTime => I128
	zdt_with_time_zone! : ZonedDateTime, TimeZone => Try(ZonedDateTime, Err)
	zdt_plain_date! : ZonedDateTime => PlainDate
	zdt_plain_time! : ZonedDateTime => PlainTime
	zdt_offset_seconds! : ZonedDateTime => I64
	## IXDTF: `2024-03-10T01:30:00-05:00[America/New_York]`.
	zdt_to_str! : ZonedDateTime => Str
	## TEST GAUGE: live resource count.
	live! : {} => I32
}

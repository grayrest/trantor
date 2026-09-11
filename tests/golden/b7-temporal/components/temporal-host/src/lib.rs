//! roc:temporal host over temporal_rs (P13). PlainDate/PlainTime/Duration
//! cross as plain records; ZonedDateTime/TimeZone/Calendar are resources (P5),
//! this crate being the sole vendor of temporal_rs natives (H0c). Blocking,
//! pure computation. Owned-argument rule: RocStr args are decref'd; handles
//! are touched only through resource::with.
use core::mem::ManuallyDrop;
use trantor_abi as abi;
use abi::*;
use temporal_rs::options::{DifferenceSettings, DisplayCalendar, DisplayOffset, DisplayTimeZone, Overflow, ToStringRoundingOptions};
use temporal_rs::{Calendar, Duration, PlainDate, TemporalError, TimeZone, ZonedDateTime};

type PlainDateRec = AnonStruct1af1a0fdd5cc23ac;
type PlainTimeRec = AnonStruct2c5a54cbbebfb285;
type DurationRec = AnonStructCa37d39356fbfd17;
type Err = OtherOrRangeOrSyntaxOrTypeErr;
const NANOS_PER_SECOND: i64 = 1_000_000_000;

/// temporal_rs 0.2.6 keeps `ErrorKind` private to the error; classify on its
/// Debug rendering (`TemporalError { kind: Range, .. }`).
fn to_err(e: TemporalError) -> Err {
    use OtherOrRangeOrSyntaxOrTypeErrTag as T;
    let dbg = format!("{e:?}");
    let tag = if dbg.contains("Range") { T::Range } else if dbg.contains("Syntax") { T::Syntax } else if dbg.contains("Type") { T::TypeErr } else { T::Other };
    let msg = ManuallyDrop::new(RocStr::from_str(&format!("{e}"), abi::host()));
    let payload = match tag { T::Range => OtherOrRangeOrSyntaxOrTypeErrPayload { range: msg }, T::Syntax => OtherOrRangeOrSyntaxOrTypeErrPayload { syntax: msg }, T::TypeErr => OtherOrRangeOrSyntaxOrTypeErrPayload { type_err: msg }, T::Other => OtherOrRangeOrSyntaxOrTypeErrPayload { other: msg } };
    Err { payload, tag }
}
fn take_str(s: RocStr) -> String { let v = s.as_str().to_string(); unsafe { s.decref(abi::host()) }; v }
fn handle<T: 'static>(v: T) -> *mut u64 { abi::resource::new(v) as *mut u64 }
fn cal(c: *mut u64) -> Calendar { unsafe { abi::resource::with(c as RocBox, |x: &mut Calendar| x.clone()) } }
fn tz(t: *mut u64) -> TimeZone { unsafe { abi::resource::with(t as RocBox, |x: &mut TimeZone| *x) } }
fn with_zdt<R>(z: *mut u64, f: impl FnOnce(&ZonedDateTime) -> R) -> R { unsafe { abi::resource::with(z as RocBox, |x: &mut ZonedDateTime| f(x)) } }

fn date_of(r: PlainDateRec, c: Calendar) -> Result<PlainDate, TemporalError> { PlainDate::try_new(r.year, r.month, r.day, c) }
fn rec_of(d: &PlainDate) -> PlainDateRec { PlainDateRec { year: d.year(), month: d.month(), day: d.day() } }
fn duration_of(r: DurationRec) -> Result<Duration, TemporalError> {
    Duration::new(r.years, r.months, r.weeks, r.days, r.hours, r.minutes, r.seconds, r.milliseconds, r.microseconds as i128, r.nanoseconds as i128)
}
fn duration_rec(d: &Duration) -> DurationRec {
    DurationRec { years: d.years(), months: d.months(), weeks: d.weeks(), days: d.days(), hours: d.hours(), minutes: d.minutes(), seconds: d.seconds(), milliseconds: d.milliseconds(), microseconds: d.microseconds() as i64, nanoseconds: d.nanoseconds() as i64 }
}

macro_rules! try_result { ($R:ident, $P:ident, $T:ident, $r:expr) => {
    match $r { Ok(v) => $R { payload: $P { ok: ManuallyDrop::new(v) }, tag: $T::Ok }, Err(e) => $R { payload: $P { err: ManuallyDrop::new(to_err(e)) }, tag: $T::Err } }
}}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__calendar_from_id(id: RocStr) -> TemporalCalendarFromIdResult {
    let id = take_str(id);
    try_result!(TemporalCalendarFromIdResult, TemporalCalendarFromIdResultPayload, TemporalCalendarFromIdResultTag, Calendar::try_from_utf8(id.as_bytes()).map(handle))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__calendar_id(c: *mut u64) -> RocStr { RocStr::from_str(cal(c).identifier(), abi::host()) }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__time_zone_from_id(id: RocStr) -> TemporalTimeZoneFromIdResult {
    let id = take_str(id);
    try_result!(TemporalTimeZoneFromIdResult, TemporalTimeZoneFromIdResultPayload, TemporalTimeZoneFromIdResultTag, TimeZone::try_from_str(&id).map(handle))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__time_zone_id(t: *mut u64) -> RocStr {
    RocStr::from_str(&tz(t).identifier().unwrap_or_default(), abi::host())
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__date_add(d: PlainDateRec, dur: DurationRec, c: *mut u64) -> TemporalDateAddResult {
    let r = date_of(d, cal(c)).and_then(|d| duration_of(dur).and_then(|dur| d.add(&dur, Some(Overflow::Constrain)))).map(|d| rec_of(&d));
    try_result!(TemporalDateAddResult, TemporalDateAddResultPayload, TemporalDateAddResultTag, r)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__date_until(a: PlainDateRec, b: PlainDateRec, c: *mut u64) -> TemporalDateUntilResult {
    let calendar = cal(c);
    let r = date_of(a, calendar.clone()).and_then(|a| date_of(b, calendar).and_then(|b| a.until(&b, DifferenceSettings::default()))).map(|d| duration_rec(&d));
    try_result!(TemporalDateUntilResult, TemporalDateUntilResultPayload, TemporalDateUntilResultTag, r)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__date_day_of_week(d: PlainDateRec, c: *mut u64) -> TemporalDateDayOfWeekResult {
    let r = date_of(d, cal(c)).map(|d| d.day_of_week() as u8);
    try_result!(TemporalDateDayOfWeekResult, TemporalDateDayOfWeekResultPayload, TemporalDateDayOfWeekResultTag, r)
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_from_epoch_ns(ns: i128, t: *mut u64) -> TemporalZdtFromEpochNsResult {
    try_result!(TemporalZdtFromEpochNsResult, TemporalZdtFromEpochNsResultPayload, TemporalZdtFromEpochNsResultTag, ZonedDateTime::try_new(ns, tz(t), Calendar::ISO).map(handle))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_epoch_ns(z: *mut u64) -> i128 { with_zdt(z, |z| z.epoch_nanoseconds().as_i128()) }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_with_time_zone(z: *mut u64, t: *mut u64) -> TemporalZdtFromEpochNsResult {
    let target = tz(t);
    try_result!(TemporalZdtFromEpochNsResult, TemporalZdtFromEpochNsResultPayload, TemporalZdtFromEpochNsResultTag, with_zdt(z, |z| z.with_timezone(target)).map(handle))
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_plain_date(z: *mut u64) -> PlainDateRec { with_zdt(z, |z| rec_of(&z.to_plain_date())) }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_plain_time(z: *mut u64) -> PlainTimeRec {
    with_zdt(z, |z| { let t = z.to_plain_time(); PlainTimeRec { hour: t.hour(), minute: t.minute(), second: t.second(), millisecond: t.millisecond(), microsecond: t.microsecond(), nanosecond: t.nanosecond() } })
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_offset_seconds(z: *mut u64) -> i64 { with_zdt(z, |z| z.offset_nanoseconds() / NANOS_PER_SECOND) }
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__zdt_to_str(z: *mut u64) -> RocStr {
    let s = with_zdt(z, |z| z.to_ixdtf_string(DisplayOffset::Auto, DisplayTimeZone::Auto, DisplayCalendar::Auto, ToStringRoundingOptions::default()).unwrap_or_default());
    RocStr::from_str(&s, abi::host())
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__temporal_host__live() -> i32 { abi::resource::live() as i32 }

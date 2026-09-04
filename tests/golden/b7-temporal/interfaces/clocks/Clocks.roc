Clocks :: [].{
	## Nanoseconds since the Unix epoch (basic-cli's utc_now shape; U128).
	wall_now! : {} => Try(U128, [ClockBeforeEpoch])
	## Monotonic nanoseconds (arbitrary origin).
	monotonic_now! : {} => U64
	sleep_millis! : U64 => {}
}

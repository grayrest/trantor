app [main!] { pf: platform "../target/hematite/sqlite-unsound/platform/main.roc" }

import pf.Sql
import pf.Report

## SQ1: the roc:sqlite-unsound generic-state fold over rusqlite, exercised with
## NON-RETAINING reducers only (S7) — each consumes the borrowed cells in place
## (sum, concat-copy, predicate-count) and retains nothing, so it is correct on
## today's compiler. `fold -> List(record-with-Str)` is the clone-on-incref
## target and is NOT run here.
db = "/tmp/hematite-sq1.db"

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Sql.sql_exec!({ db, sql: "DROP TABLE IF EXISTS t", params: [] }) ? |_| Exit(1)
	Sql.sql_exec!({ db, sql: "CREATE TABLE t (id INTEGER, name TEXT, score REAL)", params: [] }) ? |_| Exit(2)
	Sql.sql_exec!({ db, sql: "INSERT INTO t (id, name, score) VALUES (1,'alice',9.5),(2,'amy',7.0),(3,'bob',4.0)", params: [] }) ? |_| Exit(3)

	# (1) aggregate: sum the id column into an I64.
	sum_box = Sql.sql_fold!({ db, sql: "SELECT id FROM t ORDER BY id", params: [] }, Box.box(0), Box.box(sum_first_int)) ? |_| Exit(4)
	Report.print!(Str.concat("sum-id: ", I64.to_str(Box.unbox(sum_box))))

	# (2) concat Text cells into one growing Str — Str.concat copies the borrow
	# out, so nothing borrowed is retained.
	names_box = Sql.sql_fold!({ db, sql: "SELECT name FROM t ORDER BY id", params: [] }, Box.box(""), Box.box(concat_first_text)) ? |_| Exit(5)
	Report.print!(Str.concat("names: ", Box.unbox(names_box)))

	# (3) predicate count over Text (reads the borrow, keeps only a count).
	acount_box = Sql.sql_fold!({ db, sql: "SELECT name FROM t ORDER BY id", params: [] }, Box.box(0), Box.box(count_a_names)) ? |_| Exit(6)
	Report.print!(Str.concat("a-names: ", I64.to_str(Box.unbox(acount_box))))

	# (4) parameter binding + Real cells: filter score>=5.0 via a bound param,
	# then count the ones >= 8.0 in the reducer (reads the Real, retains nothing).
	hi_box = Sql.sql_fold!({ db, sql: "SELECT score FROM t WHERE score >= ?1 ORDER BY id", params: [Real(5.0)] }, Box.box(0), Box.box(count_high_scores)) ? |_| Exit(7)
	Report.print!(Str.concat("hi-scores: ", I64.to_str(Box.unbox(hi_box))))

	Ok({})
}

sum_first_int : Box(I64), List(Sql.SqlValue) -> Box(I64)
sum_first_int = |acc, row| {
	add = match List.first(row) {
		Ok(Integer(n)) => n
		_ => 0
	}
	Box.box(Box.unbox(acc) + add)
}

concat_first_text : Box(Str), List(Sql.SqlValue) -> Box(Str)
concat_first_text = |acc, row| {
	s = match List.first(row) {
		Ok(Text(t)) => t
		_ => ""
	}
	prev = Box.unbox(acc)
	# Every branch copies the borrowed cell out (concat allocates); nothing
	# borrowed is retained into the accumulator.
	joined = if Str.is_empty(prev) { Str.concat(prev, s) } else { Str.concat(prev, Str.concat(",", s)) }
	Box.box(joined)
}

count_a_names : Box(I64), List(Sql.SqlValue) -> Box(I64)
count_a_names = |acc, row| {
	inc = match List.first(row) {
		Ok(Text(t)) => if Str.starts_with(t, "a") { 1 } else { 0 }
		_ => 0
	}
	Box.box(Box.unbox(acc) + inc)
}

count_high_scores : Box(I64), List(Sql.SqlValue) -> Box(I64)
count_high_scores = |acc, row| {
	inc = match List.first(row) {
		Ok(Real(f)) => if f >= 8.0 { 1 } else { 0 }
		_ => 0
	}
	Box.box(Box.unbox(acc) + inc)
}

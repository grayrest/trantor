app [main!] { pf: platform "../platform/main.roc" }
import pf.Sql
import pf.Turso
import pf.Report

## SQ4: a Roc closure registered as a turso SQL scalar (roc:turso), called from
## a SELECT and from a CREATE TRIGGER body — the "Roc-handled" feature. Integer
## in/out keeps the demo free of borrowed-cell decref intricacy.
db = ":memory:"

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Turso.turso_register_scalar!("roc_triple", Box.box(triple)) ? |_| Exit(1)
	Sql.sql_exec!({ db, sql: "CREATE TABLE t (id INTEGER)", params: [] }) ? |_| Exit(2)
	Sql.sql_exec!({ db, sql: "INSERT INTO t VALUES (1),(2),(5)", params: [] }) ? |_| Exit(3)

	# (a) call the Roc scalar from a SELECT.
	q = Sql.sql_fold!({ db, sql: "SELECT roc_triple(id) FROM t ORDER BY id", params: [] }, Box.box(0), Box.box(sum_int)) ? |_| Exit(4)
	Report.print!(Str.concat("triple-sum: ", I64.to_str(Box.unbox(q))))

	# (b) call the Roc scalar from a TRIGGER body.
	Sql.sql_exec!({ db, sql: "CREATE TABLE tripled (v INTEGER)", params: [] }) ? |_| Exit(5)
	Sql.sql_exec!({ db, sql: "CREATE TRIGGER trg AFTER INSERT ON t BEGIN INSERT INTO tripled VALUES (roc_triple(NEW.id)); END", params: [] }) ? |_| Exit(6)
	Sql.sql_exec!({ db, sql: "INSERT INTO t VALUES (10)", params: [] }) ? |_| Exit(7)
	t = Sql.sql_fold!({ db, sql: "SELECT v FROM tripled", params: [] }, Box.box(0), Box.box(sum_int)) ? |_| Exit(8)
	Report.print!(Str.concat("trigger-val: ", I64.to_str(Box.unbox(t))))
	Ok({})
}

triple : List(Sql.SqlValue) -> Sql.SqlValue
triple = |args| {
	match List.first(args) {
		Ok(Integer(n)) => Integer(n * 3)
		_ => Null
	}
}

sum_int : Box(I64), List(Sql.SqlValue) -> Box(I64)
sum_int = |acc, row| {
	add = match List.first(row) {
		Ok(Integer(n)) => n
		_ => 0
	}
	Box.box(Box.unbox(acc) + add)
}

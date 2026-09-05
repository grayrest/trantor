app [main!] { pf: platform "../platform/main.roc" }

import pf.Sql
import pf.Report

## SQ5 — the clone-on-incref TARGET. NOT ASSERTED (and not run) today.
##
## This is the ergonomic decode the whole namespace is aiming at: fold rows
## straight into a `List` of records. `collect_person` RETAINS each borrowed
## `name` cell into the accumulator (`List.append(acc, { id, name })`).
##
## Today that dangles: the borrowed `name` points into the live engine cell
## buffer, which the NEXT step overwrites and `finish` frees — so after the fold
## the retained Strs read garbage (or crash). That is exactly the one hazard
## `roc:sqlite-unsound` is named for, and exactly what upstream **clone-on-incref**
## closes: an incref of a borrowed cell will deep-copy it, making `append(name)`
## own its bytes with NO code change here.
##
## WHEN clone-on-incref LANDS: this runs correctly and verify.sh's SQ5 section can
## drop the `roc check`-only guard and assert the output is:
##     rows: 1:alice,2:amy,3:bob
## Until then this file only type-checks — proving the API SHAPE supports
## record decode — and is deliberately not executed.
db = ":memory:"

Person : { id : I64, name : Str }

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Sql.sql_exec!({ db, sql: "CREATE TABLE t (id INTEGER, name TEXT)", params: [] }) ? |_| Exit(1)
	Sql.sql_exec!({ db, sql: "INSERT INTO t VALUES (1,'alice'),(2,'amy'),(3,'bob')", params: [] }) ? |_| Exit(2)
	people_box = Sql.sql_fold!({ db, sql: "SELECT id, name FROM t ORDER BY id", params: [] }, Box.box([]), Box.box(collect_person)) ? |_| Exit(3)
	Report.print!(Str.concat("rows: ", render(Box.unbox(people_box))))
	Ok({})
}

## Retains the borrowed `name` into the accumulator — the unsound-today move that
## clone-on-incref makes sound.
collect_person : Box(List(Person)), List(Sql.SqlValue) -> Box(List(Person))
collect_person = |acc, row| {
	id = match List.get(row, 0) {
		Ok(Integer(n)) => n
		_ => 0
	}
	name = match List.get(row, 1) {
		Ok(Text(t)) => t
		_ => ""
	}
	Box.box(List.append(Box.unbox(acc), { id, name }))
}

render : List(Person) -> Str
render = |people| render_go(people, 0, "")

render_go : List(Person), U64, Str -> Str
render_go = |people, i, acc| {
	if i >= List.len(people) {
		acc
	} else {
		p = List.get(people, i) ?? { id: 0, name: "" }
		cell = Str.concat(I64.to_str(p.id), Str.concat(":", p.name))
		next = if Str.is_empty(acc) { cell } else { Str.concat(acc, Str.concat(",", cell)) }
		render_go(people, i + 1, next)
	}
}

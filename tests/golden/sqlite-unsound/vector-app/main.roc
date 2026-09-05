app [main!] { pf: platform "../platform/main.roc" }
import pf.Sql
import pf.Report

## SQ3 vector demo (base SQL, no Roc leaf): turso's vector32()/vector_distance_cos
## order rows by cosine similarity; rusqlite rejects vector32 (the substitution's
## negative half). Same app, both worlds.
db = ":memory:"

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Sql.sql_exec!({ db, sql: "CREATE TABLE docs (id INTEGER, name TEXT, emb BLOB)", params: [] }) ?? {}
	result = match Sql.sql_exec!({ db, sql: "INSERT INTO docs VALUES (1,'near',vector32('[1.0,0.0,0.0]')),(2,'mid',vector32('[0.7,0.7,0.0]')),(3,'far',vector32('[0.0,1.0,0.0]'))", params: [] }) {
		Err(_) => "unsupported"
		Ok({}) => {
			ordered = Sql.sql_fold!({ db, sql: "SELECT name FROM docs ORDER BY vector_distance_cos(emb, vector32('[1.0,0.0,0.0]'))", params: [] }, Box.box(""), Box.box(concat_name)) ?? Box.box("query-failed")
			Box.unbox(ordered)
		}
	}
	Report.print!(Str.concat("vector: ", result))
	Ok({})
}

concat_name : Box(Str), List(Sql.SqlValue) -> Box(Str)
concat_name = |acc, row| {
	s = match List.first(row) {
		Ok(Text(t)) => t
		_ => ""
	}
	prev = Box.unbox(acc)
	Box.box(if Str.is_empty(prev) { Str.concat(prev, s) } else { Str.concat(prev, Str.concat(",", s)) })
}

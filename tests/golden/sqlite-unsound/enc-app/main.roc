app [main!] { pf: platform "../target/hematite/sqlite-unsound/platform/main.roc" }
import pf.Sql
import pf.Report

## SQ3 encryption demo (turso, host-side key from env — no Roc leaf): writes a
## secret to a FILE db and reads it back correctly through the same interface,
## while the on-disk file is ciphertext (verify greps the plaintext out).
db = "/tmp/hematite-sq3-enc.db"

main! : {} => Try({}, [Exit(I32), ..])
main! = |{}| {
	Sql.sql_exec!({ db, sql: "DROP TABLE IF EXISTS secrets", params: [] }) ? |_| Exit(1)
	Sql.sql_exec!({ db, sql: "CREATE TABLE secrets (id INTEGER, val TEXT)", params: [] }) ? |_| Exit(2)
	Sql.sql_exec!({ db, sql: "INSERT INTO secrets VALUES (1,'topsecret-alice')", params: [] }) ? |_| Exit(3)
	got = Sql.sql_fold!({ db, sql: "SELECT val FROM secrets", params: [] }, Box.box(""), Box.box(first_text)) ? |_| Exit(4)
	Report.print!(Str.concat("enc-read: ", Box.unbox(got)))
	Ok({})
}

first_text : Box(Str), List(Sql.SqlValue) -> Box(Str)
first_text = |acc, row| {
	s = match List.first(row) {
		Ok(Text(t)) => t
		_ => ""
	}
	Box.box(Str.concat(Box.unbox(acc), s))
}

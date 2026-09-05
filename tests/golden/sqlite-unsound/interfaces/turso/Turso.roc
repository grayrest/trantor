import Sql
## roc:turso — Roc-handled SQL functions. Register a Roc closure under a SQL
## name; thereafter any query or trigger that calls that name invokes the Roc
## function (its borrowed SqlValue args obey the same non-retain rule as fold
## rows). The "Roc-handled trigger" feature.
Turso :: [].{
	turso_register_scalar! : Str, Box((List(Sql.SqlValue) -> Sql.SqlValue)) => Try({}, Str)
}

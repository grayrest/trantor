## roc:sqlite-unsound: a fully-encapsulated cursor fold. `sql_exec!` runs a
## write/DDL to completion; `sql_fold!` drives an internal iteration, handing
## each row (a `List(SqlValue)` with BORROWED Text/Blob cells) to a pure reducer
## over a boxed accumulator. Retaining a borrowed cell past its reducer call
## dangles until upstream clone-on-incref — hence `-unsound`.
Sql :: [].{
	SqlValue : [Null, Integer(I64), Real(F64), Text(Str), Blob(List(U8))]
	sql_exec! : { db : Str, sql : Str, params : List(SqlValue) } => Try({}, Str)
	sql_fold! : { db : Str, sql : Str, params : List(SqlValue) }, Box(state), Box((Box(state), List(SqlValue) -> Box(state))) => Try(Box(state), Str)
}

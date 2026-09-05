//! roc:sqlite-unsound over rusqlite (bundled libsqlite3). Two encapsulated
//! leaves: `sql_exec!` runs a write/DDL to completion; `sql_fold!` drives an
//! internal iteration, building each row as a `List(SqlValue)` whose Text/Blob
//! cells BORROW the live sqlite column buffer (zero copy, valid only until the
//! next step) and handing it to a boxed Roc reducer over a boxed accumulator.
//! rusqlite-host is the sole vendor of libsqlite3 (H0c). Single-threaded model
//! (P12): the connection lock is held across the fold, reducer calls included.
use core::mem::{ManuallyDrop, MaybeUninit};
use hematite_abi as abi;
use abi::{
    AnonStruct2aa6240abf9d9e42 as FoldArgsIn, BlobOrIntegerOrNullOrRealOrText as Cell,
    BlobOrIntegerOrNullOrRealOrTextPayload as CellPayload,
    BlobOrIntegerOrNullOrRealOrTextTag as CellTag, RocBox, RocErasedCallable, RocHost, RocList,
    RocStr, SqlSqlExecArgs, SqlSqlExecResult, SqlSqlExecResultPayload, SqlSqlExecResultTag,
    SqlSqlFoldResult, SqlSqlFoldResultPayload, SqlSqlFoldResultTag,
};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{params_from_iter, Connection};
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-global connection cache, keyed by db string (S4). rusqlite's
/// `Connection` is `!Sync`; the `Mutex` serializes access, and the single-thread
/// model means only one fold/exec is ever in flight.
static CONNS: Mutex<Option<HashMap<String, Connection>>> = Mutex::new(None);

/// Convert the app's params (owned SqlValue cells) into owned rusqlite values,
/// copying Text/Blob out so they can be bound after the args are released.
unsafe fn params_to_values(list: &RocList<Cell>) -> Vec<Value> {
    list.as_slice()
        .iter()
        .map(|c| match c.tag {
            CellTag::Null => Value::Null,
            CellTag::Integer => Value::Integer(*c.borrow_payload_integer_unchecked()),
            CellTag::Real => Value::Real(*c.borrow_payload_real_unchecked()),
            CellTag::Text => Value::Text(c.borrow_payload_text_unchecked().as_str().to_string()),
            CellTag::Blob => Value::Blob(c.borrow_payload_blob_unchecked().as_slice().to_vec()),
        })
        .collect()
}

/// Build the current row as a `List(SqlValue)` whose Text/Blob cells BORROW the
/// live sqlite column bytes (valid only until the next step). Integer/Real/Null
/// are inline; only the spine is allocated (rc=1, freed by the reducer's decref).
unsafe fn build_row(row: &rusqlite::Row, ncols: usize, host: &RocHost) -> RocList<Cell> {
    let list = RocList::<Cell>::allocate(ncols, host);
    for i in 0..ncols {
        let cell = match row.get_ref(i) {
            Ok(ValueRef::Text(b)) => Cell {
                payload: CellPayload { text: ManuallyDrop::new(abi::borrow::borrowed_str(b.as_ptr(), b.len())) },
                tag: CellTag::Text,
            },
            Ok(ValueRef::Blob(b)) => Cell {
                payload: CellPayload { blob: ManuallyDrop::new(abi::borrow::borrowed_bytes(b.as_ptr(), b.len())) },
                tag: CellTag::Blob,
            },
            Ok(ValueRef::Integer(n)) => Cell {
                payload: CellPayload { integer: ManuallyDrop::new(n) },
                tag: CellTag::Integer,
            },
            Ok(ValueRef::Real(f)) => Cell {
                payload: CellPayload { real: ManuallyDrop::new(f) },
                tag: CellTag::Real,
            },
            _ => Cell { payload: CellPayload { null: [] }, tag: CellTag::Null },
        };
        list.elements.add(i).write(cell);
    }
    list
}

/// Reducer args in Roc parameter order `(acc, row)`; the erased-callable ABI
/// consumes both and writes the new accumulator to the return slot.
#[repr(C)]
struct FoldArgs {
    arg0: RocBox,
    arg1: RocList<Cell>,
}

/// Invoke the boxed reducer `(Box(state), row) -> Box(state)` once, consuming
/// `acc` and `row`, returning the new boxed accumulator.
unsafe fn invoke_reducer(reducer: RocErasedCallable, acc: RocBox, row: RocList<Cell>, host: &RocHost) -> RocBox {
    let args = FoldArgs { arg0: acc, arg1: row };
    let mut ret = MaybeUninit::<RocBox>::uninit();
    let payload = abi::roc_erased_callable_payload_ptr(reducer);
    let capture = abi::roc_erased_callable_capture_ptr(reducer);
    ((*payload).callable_fn_ptr)(
        host as *const RocHost as *mut RocHost,
        ret.as_mut_ptr() as *mut u8,
        &args as *const FoldArgs as *const u8,
        capture,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    ret.assume_init()
}

fn exec_ok() -> SqlSqlExecResult {
    SqlSqlExecResult { payload: SqlSqlExecResultPayload { ok: [] }, tag: SqlSqlExecResultTag::Ok }
}
fn exec_err(msg: &str) -> SqlSqlExecResult {
    SqlSqlExecResult { payload: SqlSqlExecResultPayload { err: ManuallyDrop::new(RocStr::from_str(msg, abi::host())) }, tag: SqlSqlExecResultTag::Err }
}
fn fold_ok(state: RocBox) -> SqlSqlFoldResult {
    SqlSqlFoldResult { payload: SqlSqlFoldResultPayload { ok: ManuallyDrop::new(state) }, tag: SqlSqlFoldResultTag::Ok }
}
fn fold_err(msg: &str) -> SqlSqlFoldResult {
    SqlSqlFoldResult { payload: SqlSqlFoldResultPayload { err: ManuallyDrop::new(RocStr::from_str(msg, abi::host())) }, tag: SqlSqlFoldResultTag::Err }
}

/// `Sql.sql_exec! : { db, sql, params } => Try({}, Str)`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__rusqlite_host__sql_exec(a: SqlSqlExecArgs) -> SqlSqlExecResult {
    let db = a.db.as_str().to_string();
    let sql = a.sql.as_str().to_string();
    let values = unsafe { params_to_values(&a.params) };
    unsafe { a.decref(abi::host()); } // owned args released after copying out

    let mut guard = CONNS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if !map.contains_key(&db) {
        match Connection::open(&db) {
            Ok(c) => { map.insert(db.clone(), c); }
            Err(e) => return exec_err(&e.to_string()),
        }
    }
    let conn = map.get(&db).unwrap();
    match conn.prepare(&sql).and_then(|mut stmt| stmt.execute(params_from_iter(values)).map(|_| ())) {
        Ok(()) => exec_ok(),
        Err(e) => exec_err(&e.to_string()),
    }
}

/// `Sql.sql_fold! : { db, sql, params }, Box(state), Box((Box(state), List(SqlValue) -> Box(state))) => Try(Box(state), Str)`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn hematite__rusqlite_host__sql_fold(a: FoldArgsIn, initial: RocBox, reducer: RocErasedCallable) -> SqlSqlFoldResult {
    let host = abi::host();
    let db = a.db.as_str().to_string();
    let sql = a.sql.as_str().to_string();
    let values = unsafe { params_to_values(&a.params) };
    unsafe { a.decref(host); }

    let mut acc = initial;
    let result: Result<RocBox, String> = (|| {
        let mut guard = CONNS.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        if !map.contains_key(&db) {
            let c = Connection::open(&db).map_err(|e| e.to_string())?;
            map.insert(db.clone(), c);
        }
        let conn = map.get(&db).unwrap();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let ncols = stmt.column_count();
        let mut rows = stmt.query(params_from_iter(values)).map_err(|e| e.to_string())?;
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    // The row borrows the live statement buffer; build the
                    // borrowed cells and hand them to the reducer BEFORE the next
                    // step invalidates them.
                    let roc_row = unsafe { build_row(row, ncols, host) };
                    acc = unsafe { invoke_reducer(reducer, acc, roc_row, host) };
                }
                Ok(None) => break Ok(acc),
                Err(e) => break Err(e.to_string()),
            }
        }
    })();

    // The fold owns the reducer (owned hosted-fn arg): release it once.
    unsafe { abi::decref_erased_callable(reducer, host); }
    match result {
        Ok(state) => fold_ok(state),
        // On the rare error path the accumulator-so-far (an opaque `Box(state)`)
        // is deliberately leaked rather than decref'd: `decref_box` can't reach
        // the opaque inner refs, so leaking is the safe choice for an error.
        Err(msg) => {
            let _ = acc;
            fold_err(&msg)
        }
    }
}

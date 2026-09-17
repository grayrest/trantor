//! roc:sqlite-unsound over turso (turso_sdk_kit, pure-Rust engine) — the SECOND
//! backend for the SAME interface (the H5 substitution thesis at SQL scale).
//! turso's `row_value(i)` returns an OWNED Value, so the fold holds the row's
//! Values alive across the reducer call and BORROWS Text/Blob from them: same
//! zero-copy, same unsound-if-retained property as the rusqlite backend, just a
//! different lifetime source. turso-host is the sole vendor of the turso engine
//! (pure Rust — no libsqlite3). Blocking (`async_io=false`, P12).
use core::mem::{ManuallyDrop, MaybeUninit};
use trantor_abi as abi;
use abi::{
    AnonStruct2aa6240abf9d9e42 as FoldArgsIn, BlobOrIntegerOrNullOrRealOrText as Cell,
    BlobOrIntegerOrNullOrRealOrTextPayload as CellPayload,
    BlobOrIntegerOrNullOrRealOrTextTag as CellTag, RocBox, RocErasedCallable, RocHost, RocList,
    RocStr, SqlSqlExecArgs, SqlSqlExecResult, SqlSqlExecResultPayload, SqlSqlExecResultTag,
    SqlSqlFoldResult, SqlSqlFoldResultPayload, SqlSqlFoldResultTag,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use turso_sdk_kit::rsapi::{
    ContextDestructor, EncryptionOpts, ExtensionValue, TursoConnection, TursoDatabase,
    TursoDatabaseConfig, TursoStatusCode, Value, ValueDestructor,
};

/// Roc closures registered as SQL scalar functions (roc:turso, S10): (name,
/// closure-pointer-as-usize). The closure is BORROWED on each call (never
/// decref'd) and lives for the process; the registry owns it. Re-registered on
/// every fresh connection so a per-call connect still sees them.
static SCALARS: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());

/// Register every recorded Roc scalar on a freshly-opened connection.
fn register_scalars(conn: &TursoConnection) {
    for (name, ctx) in SCALARS.lock().unwrap().iter() {
        // argc -1 = variadic; deterministic false; `ctx` carries the Roc closure
        // pointer to the trampoline; no destructors (the closure outlives calls).
        let _ = conn.register_external_scalar_function(name.clone(), -1, false, *ctx, roc_scalar_trampoline, None, None);
    }
}

/// One argument in Roc order for the scalar closure `List(SqlValue) -> SqlValue`.
#[repr(C)]
struct ScalarArgs {
    arg0: RocList<Cell>,
}

/// turso invokes this per SQL call of a registered Roc scalar. `context` is the
/// Roc closure pointer. Build the args as a borrowed `List(SqlValue)`, invoke
/// the closure (borrowed — never decref'd), convert its returned SqlValue to a
/// turso value. Runs INSIDE the VDBE (re-entrant, R-SQ4).
unsafe extern "C" fn roc_scalar_trampoline(
    context: usize,
    argc: i32,
    argv: *const ExtensionValue,
    _cd: Option<ContextDestructor>,
    _vd: Option<ValueDestructor>,
) -> ExtensionValue {
    let host = abi::host();
    let closure = context as RocErasedCallable;
    let args = std::slice::from_raw_parts(argv, argc.max(0) as usize);

    let list = RocList::<Cell>::allocate(args.len(), host);
    for (i, v) in args.iter().enumerate() {
        // Text is borrowed from the live turso value (valid during this call);
        // Integer/Real are inline. to_integer is the type check (no coercion).
        let cell = if let Some(n) = v.to_integer() {
            Cell { payload: CellPayload { integer: ManuallyDrop::new(n) }, tag: CellTag::Integer }
        } else if let Some(s) = v.to_text() {
            Cell { payload: CellPayload { text: ManuallyDrop::new(abi::borrow::borrowed_str(s.as_ptr(), s.len())) }, tag: CellTag::Text }
        } else if let Some(f) = v.to_float() {
            Cell { payload: CellPayload { real: ManuallyDrop::new(f) }, tag: CellTag::Real }
        } else {
            Cell { payload: CellPayload { null: [] }, tag: CellTag::Null }
        };
        list.elements.add(i).write(cell);
    }

    // Invoke the closure `(row) -> SqlValue` (borrow the closure: null reuse).
    let call_args = ScalarArgs { arg0: list };
    let mut ret = MaybeUninit::<Cell>::uninit();
    let payload = abi::roc_erased_callable_payload_ptr(closure);
    let capture = abi::roc_erased_callable_capture_ptr(closure);
    ((*payload).callable_fn_ptr)(
        host as *const RocHost as *mut RocHost,
        ret.as_mut_ptr() as *mut u8,
        &call_args as *const ScalarArgs as *const u8,
        capture,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    let cell = ret.assume_init();

    // Convert the returned SqlValue to a turso value, releasing a refcounted
    // Text/Blob payload after copying it out.
    match cell.tag {
        CellTag::Integer => ExtensionValue::from_integer(*cell.borrow_payload_integer_unchecked()),
        CellTag::Real => ExtensionValue::from_float(*cell.borrow_payload_real_unchecked()),
        CellTag::Text => {
            let s = cell.borrow_payload_text_unchecked().as_str().to_string();
            (*(&cell as *const Cell as *mut Cell)).take_payload_text_unchecked().decref(host);
            ExtensionValue::from_text(s)
        }
        CellTag::Blob => {
            (*(&cell as *const Cell as *mut Cell)).take_payload_blob_unchecked().decref(host);
            ExtensionValue::null()
        }
        CellTag::Null => ExtensionValue::null(),
    }
}

/// `Turso.turso_register_scalar! : Str, Box((List(SqlValue) -> SqlValue)) => Try({}, Str)`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__turso_host__turso_register_scalar(name: RocStr, closure: RocErasedCallable) -> abi::TursoTursoRegisterScalarResult {
    // Record the closure (kept alive for the process; the registry owns it, so
    // it is NOT decref'd here). The name is copied out then released.
    SCALARS.lock().unwrap().push((name.as_str().to_string(), closure as usize));
    unsafe { name.decref(abi::host()); }
    abi::TursoTursoRegisterScalarResult {
        payload: abi::TursoTursoRegisterScalarResultPayload { ok: [] },
        tag: abi::TursoTursoRegisterScalarResultTag::Ok,
    }
}

// turso pulls `iana_time_zone`, which needs macOS CoreFoundation. It declares
// that via a build-script directive (`cargo:rustc-link-lib`), which is lost when
// the crate is bundled into a staticlib — roc's linker never sees it. Emitting
// an explicit `#[link]` here puts an LC_LINKER_OPTION load command into this
// archive, which roc's ld64 honors (the same way temporal_host's CF deps link).
// macOS only: `iana_time_zone` reads /etc/localtime elsewhere, and rustc refuses
// a framework link on any other target.
#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {}

/// Process-global open-database cache, keyed by db string (S4).
static DBS: Mutex<Option<HashMap<String, Arc<TursoDatabase>>>> = Mutex::new(None);

fn get_db(path: &str) -> Result<Arc<TursoDatabase>, String> {
    let mut g = DBS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(db) = map.get(path) {
        return Ok(db.clone());
    }
    // Encryption is HOST-SIDE and env-gated (S9): a deploy secret, never a Roc
    // value. With TRANTOR_TURSO_ENCRYPTION_HEXKEY set, the db is opened
    // encrypted (aes256gcm) and reads/writes are transparent to the app; the
    // on-disk file is ciphertext.
    let (experimental, encryption) = match std::env::var("TRANTOR_TURSO_ENCRYPTION_HEXKEY") {
        Ok(hexkey) if !hexkey.is_empty() => (
            Some("encryption".to_string()),
            Some(EncryptionOpts { cipher: "aes256gcm".to_string(), hexkey }),
        ),
        _ => (None, None),
    };
    let db = TursoDatabase::new(TursoDatabaseConfig {
        path: path.to_string(),
        experimental_features: experimental,
        async_io: false,
        encryption,
        vfs: None,
        io: None,
        db_file: None,
    });
    let io = db.open().map_err(|e| format!("open {path:?}: {e:?}"))?;
    if io.is_io() {
        return Err(format!("open {path:?}: Io under async_io=false"));
    }
    // TursoDatabase::new already returns an Arc-wrapped handle.
    map.insert(path.to_string(), db.clone());
    Ok(db)
}

/// Owned SqlValue cell -> turso Value (params are copied for binding).
unsafe fn to_turso(cell: &Cell) -> Value {
    match cell.tag {
        CellTag::Null => Value::Null,
        CellTag::Integer => Value::from_i64(*cell.borrow_payload_integer_unchecked()),
        CellTag::Real => Value::from_f64(*cell.borrow_payload_real_unchecked()),
        CellTag::Text => Value::build_text(cell.borrow_payload_text_unchecked().as_str().to_string()),
        CellTag::Blob => Value::Blob(cell.borrow_payload_blob_unchecked().as_slice().to_vec()),
    }
}

/// Build the current row as a `List(SqlValue)` whose Text/Blob cells BORROW from
/// the owned `values` (which the caller keeps alive across the reducer call).
unsafe fn build_row(values: &[Value], host: &RocHost) -> RocList<Cell> {
    let list = RocList::<Cell>::allocate(values.len(), host);
    for (i, v) in values.iter().enumerate() {
        let cell = match v {
            Value::Text(_) => {
                let s = v.to_text().unwrap_or("");
                Cell { payload: CellPayload { text: ManuallyDrop::new(abi::borrow::borrowed_str(s.as_ptr(), s.len())) }, tag: CellTag::Text }
            }
            Value::Blob(b) => Cell {
                payload: CellPayload { blob: ManuallyDrop::new(abi::borrow::borrowed_bytes(b.as_ptr(), b.len())) },
                tag: CellTag::Blob,
            },
            Value::Numeric(_) => {
                if let Some(n) = v.as_int() {
                    Cell { payload: CellPayload { integer: ManuallyDrop::new(n) }, tag: CellTag::Integer }
                } else {
                    Cell { payload: CellPayload { real: ManuallyDrop::new(v.to_float_or_zero()) }, tag: CellTag::Real }
                }
            }
            Value::Null => Cell { payload: CellPayload { null: [] }, tag: CellTag::Null },
        };
        list.elements.add(i).write(cell);
    }
    list
}

#[repr(C)]
struct FoldArgs {
    arg0: RocBox,
    arg1: RocList<Cell>,
}

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
pub extern "C-unwind" fn trantor__turso_host__sql_exec(a: SqlSqlExecArgs) -> SqlSqlExecResult {
    let db_s = a.db.as_str().to_string();
    let sql = a.sql.as_str().to_string();
    let values: Vec<Value> = unsafe { a.params.as_slice().iter().map(|c| to_turso(c)).collect() };
    unsafe { a.decref(abi::host()); }

    let result: Result<(), String> = (|| {
        let db = get_db(&db_s)?;
        let conn = db.connect().map_err(|e| format!("{e:?}"))?;
        register_scalars(&conn);
        let mut stmt = conn.prepare_cached(&sql).map_err(|e| format!("{e:?}"))?;
        for (i, v) in values.into_iter().enumerate() {
            stmt.bind_positional(i + 1, v).map_err(|e| format!("bind {}: {e:?}", i + 1))?;
        }
        stmt.execute(None).map_err(|e| format!("{e:?}"))?;
        Ok(())
    })();
    match result {
        Ok(()) => exec_ok(),
        Err(m) => exec_err(&m),
    }
}

/// `Sql.sql_fold! : { db, sql, params }, Box(state), Box((Box(state), List(SqlValue) -> Box(state))) => Try(Box(state), Str)`
#[unsafe(no_mangle)]
pub extern "C-unwind" fn trantor__turso_host__sql_fold(a: FoldArgsIn, initial: RocBox, reducer: RocErasedCallable) -> SqlSqlFoldResult {
    let host = abi::host();
    let db_s = a.db.as_str().to_string();
    let sql = a.sql.as_str().to_string();
    let values: Vec<Value> = unsafe { a.params.as_slice().iter().map(|c| to_turso(c)).collect() };
    unsafe { a.decref(host); }

    let mut acc = initial;
    let result: Result<RocBox, String> = (|| {
        let db = get_db(&db_s)?;
        let conn = db.connect().map_err(|e| format!("{e:?}"))?;
        register_scalars(&conn);
        let mut stmt = conn.prepare_cached(&sql).map_err(|e| format!("{e:?}"))?;
        for (i, v) in values.into_iter().enumerate() {
            stmt.bind_positional(i + 1, v).map_err(|e| format!("bind {}: {e:?}", i + 1))?;
        }
        loop {
            match stmt.step(None) {
                Ok(TursoStatusCode::Row) => {
                    let ncols = stmt.column_count();
                    // Own each cell's Value; they stay alive across the reducer
                    // call so the borrowed row is valid, then drop after.
                    let row_vals: Vec<Value> = (0..ncols).map(|i| stmt.row_value(i).unwrap_or(Value::Null)).collect();
                    let roc_row = unsafe { build_row(&row_vals, host) };
                    acc = unsafe { invoke_reducer(reducer, acc, roc_row, host) };
                    drop(row_vals);
                }
                Ok(TursoStatusCode::Done) => break Ok(acc),
                Ok(TursoStatusCode::Io) => break Err("turso Io under async_io=false".to_string()),
                Err(e) => break Err(format!("{e:?}")),
            }
        }
    })();

    unsafe { abi::decref_erased_callable(reducer, host); }
    match result {
        Ok(s) => fold_ok(s),
        Err(m) => {
            let _ = acc; // leak-on-error (opaque box), as in rusqlite-host
            fold_err(&m)
        }
    }
}

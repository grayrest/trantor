//! sync-io core (P9 shared substrate): the Stream trait and resource
//! constructors. Producers (filesystem, sockets, stdio, the test backings)
//! depend on this rlib to mint stream resources. It deliberately exports NO
//! `#[no_mangle]` symbols: cargo bundles an rlib into each dependent staticlib,
//! and a bundled no_mangle would be the H0c duplicate-symbol footgun. The
//! hosted read!/write! symbols live in the separate `sync-io` staticlib.
use hematite_abi as abi;
use abi::RocBox;
use std::io::{BufReader, Read, Write};

/// The backing of an InputStream resource. Buffered so `read_until!` (line
/// reads for files, stdin and sockets alike) has one implementation; `read!`
/// drains the buffer first, so the two compose on one stream.
pub struct Input(pub BufReader<Box<dyn Read>>);
/// The backing of an OutputStream resource.
pub struct Output(pub Box<dyn Write>);

/// Mint an InputStream resource (a refcounted Box(U64) handle, P5).
pub fn input_stream(r: Box<dyn Read>) -> RocBox { abi::resource::new(Input(BufReader::new(r))) }
/// Mint an OutputStream resource.
pub fn output_stream(w: Box<dyn Write>) -> RocBox { abi::resource::new(Output(w)) }

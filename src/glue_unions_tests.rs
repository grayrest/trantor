//! Tests for `glue_unions.rs`.
use super::*;
use crate::glue_placeholder::{with_placeholder, PLACEHOLDER};

#[test]
fn one_variant_is_found_with_its_name_and_field_count() {
    assert_eq!(only_variant("Echo := [\n\t## doc\n\tPing(U64, Str, Str),\n]", "Echo"), Some(("Ping".into(), 3)));
    assert_eq!(only_variant("Tick := [Stop]", "Tick"), Some(("Stop".into(), 0)));
    assert_eq!(only_variant("Echo := [Ping(U64), Shout(Str)]", "Echo"), None);
}

#[test]
fn the_placeholder_is_a_second_variant() {
    assert_eq!(with_placeholder("Echo := [\n\tPing(U64, Str, Str),\n]\n", "Echo").unwrap(), format!("Echo := [\n\tPing(U64, Str, Str),\n\n{PLACEHOLDER},\n]\n"));
    assert_eq!(with_placeholder("Tick := [Stop]", "Tick").unwrap(), format!("Tick := [Stop,\n{PLACEHOLDER},\n]"));
    let commented = with_placeholder("Bell := [\n\tRing(U64, Str) # id, text\n]\n", "Bell").unwrap();
    assert_eq!(commented, format!("Bell := [\n\tRing(U64, Str), # id, text\n\n{PLACEHOLDER},\n]\n"), "the comma goes before the comment");
}

#[test]
fn a_payload_with_nothing_refcounted_binds_no_unused_variable() {
    let placeholder = "#[repr(C)]\npub struct BellRingPayload {\n    pub _0: u64,\n    pub _1: u64,\n}\n";
    let s = Single { module: "Bell".into(), variant: "Ring".into(), fields: 2, block: "Cmd", wrapper: "bell".into() };
    let items = payload_items(placeholder, &s, "BellRingPayload").unwrap();
    assert!(items.contains("let _ = self;") && !items.contains("let payload = self;"), "{items}");
}

#[test]
fn only_the_blocks_own_field_and_exact_accessors_are_retyped() {
    let glue = "pub union CmdPayload {\n    pub bell: core::mem::ManuallyDrop<RocStr>,\n    pub door_bell: core::mem::ManuallyDrop<u64>,\n}\n\
                pub union EventPayload {\n    pub bell: core::mem::ManuallyDrop<RocStr>,\n}\n\
                impl Cmd {\n    pub unsafe fn take_payload_bell_unchecked(&mut self) -> RocStr {\n        x\n    }\n    pub unsafe fn take_payload_door_bell_unchecked(&mut self) -> u64 {\n        y\n    }\n\
                \x20   pub unsafe fn decref(self, roc_host: &RocHost) {\n        match value.tag {\n            CmdTag::Bell => {\n                wrong();\n            },\n        }\n    }\n}\n";
    let s = Single { module: "Bell".into(), variant: "Ring".into(), fields: 2, block: "Cmd", wrapper: "bell".into() };
    let out = retype_wrapper(glue, &s, "BellRingPayload").unwrap();
    assert!(out.contains("pub bell: core::mem::ManuallyDrop<BellRingPayload>,"));
    assert!(out.contains("pub door_bell: core::mem::ManuallyDrop<u64>,"), "{out}");
    assert_eq!(out.matches("ManuallyDrop<BellRingPayload>").count(), 1, "the Event union's `bell` is not the Cmd's: {out}");
    assert!(out.contains("take_payload_bell_unchecked(&mut self) -> BellRingPayload {") && out.contains("take_payload_door_bell_unchecked(&mut self) -> u64"));
    assert!(!out.contains("wrong()") && out.contains("payload.decref(roc_host)"), "{out}");
}

/// Glue for `VaultEvent := [Answer({ seq : U64, body : Str })]` under an `Event`
/// driver union: the record is an `AnonStruct`, and glue names the unwrapped
/// union after it itself. `extra` is appended to the glue text.
fn record_event_glue(extra: &str) -> String {
    format!(
        "pub struct AnonStruct3ea6 {{\n    pub seq: u64,\n    pub body: RocStr,\n}}\n\
         impl Event {{\n    pub unsafe fn take_payload_vault_unchecked(&mut self) -> AnonStruct3ea6 {{\n        x\n    }}\n}}\n\
         {extra}pub type VaultEventAnswer = AnonStruct3ea6;\n"
    )
}

fn record_event_single() -> Single {
    Single { module: "VaultEvent".into(), variant: "Answer".into(), fields: 1, block: "Event", wrapper: "vault".into() }
}

#[test]
fn a_one_field_record_payload_already_named_by_glue_is_not_aliased_again() {
    let out = repair(&record_event_glue("pub type VaultEvent = AnonStruct3ea6;\n"), &[record_event_single()], None).unwrap();
    assert_eq!(out.matches("pub type VaultEvent =").count(), 1, "a second alias is E0428: {out}");
}

#[test]
fn a_one_field_payload_is_aliased_when_glue_named_only_a_longer_name() {
    let out = repair(&record_event_glue(""), &[record_event_single()], None).unwrap();
    assert!(out.contains("pub type VaultEvent = AnonStruct3ea6;"), "`VaultEventAnswer` is not `VaultEvent`: {out}");
}

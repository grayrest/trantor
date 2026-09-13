//! Tests for `journal.rs`.
use super::*;

fn project(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("trantor-journal-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A journal whose owner died: the lock is released and nothing commits.
fn abandon(j: Journal) {
    drop(j);
}

#[test]
fn a_rollback_restores_bytes_and_absence() {
    let d = project("rollback");
    std::fs::write(d.join("world.toml"), "before").unwrap();
    let j = Journal::begin(&d, &["world.toml", "trantor.lock"]).unwrap();
    write_atomic(&d.join("world.toml"), b"after").unwrap();
    write_atomic(&d.join("trantor.lock"), b"created").unwrap();
    j.rollback().unwrap();
    assert_eq!(std::fs::read_to_string(d.join("world.toml")).unwrap(), "before");
    assert!(!d.join("trantor.lock").exists());
    assert!(!d.join(JOURNAL_DIR).exists());
}

#[test]
fn an_edit_interrupted_before_commit_is_undone_by_the_next_command() {
    let d = project("interrupted");
    std::fs::write(d.join("package.toml"), "before").unwrap();
    let j = Journal::begin(&d, &["package.toml", "trantor.lock"]).unwrap();
    write_atomic(&d.join("package.toml"), b"after").unwrap();
    write_atomic(&d.join("trantor.lock"), b"pinned").unwrap();
    j.written().unwrap();
    abandon(j);
    let Recovery::Restored(note) = recover(&d).unwrap() else { panic!("nothing restored") };
    assert!(note.contains("package.toml"), "{note}");
    assert_eq!(std::fs::read_to_string(d.join("package.toml")).unwrap(), "before");
    assert!(!d.join("trantor.lock").exists());
}

#[test]
fn a_file_changed_after_the_crash_is_kept_and_the_journal_stays() {
    let d = project("conflict");
    std::fs::write(d.join("world.toml"), "before").unwrap();
    let j = Journal::begin(&d, &["world.toml"]).unwrap();
    write_atomic(&d.join("world.toml"), b"after").unwrap();
    j.written().unwrap();
    abandon(j);
    std::fs::write(d.join("world.toml"), "after, then my hand edit").unwrap();
    assert!(matches!(recover(&d).unwrap(), Recovery::Conflict(_)));
    assert_eq!(std::fs::read_to_string(d.join("world.toml")).unwrap(), "after, then my hand edit");
    assert!(d.join(JOURNAL_DIR).exists());
    assert!(Journal::begin(&d, &["world.toml"]).is_err(), "a new edit does not start over a kept journal");
}

#[test]
fn a_nested_or_absolute_manifest_path_is_journaled_by_its_path() {
    let d = project("paths");
    std::fs::create_dir_all(d.join("variants")).unwrap();
    let abs = d.join("abs.toml");
    std::fs::write(d.join("variants/w.toml"), "nested").unwrap();
    std::fs::write(&abs, "absolute").unwrap();
    let j = Journal::begin(&d, &["variants/w.toml", abs.to_str().unwrap()]).unwrap();
    write_atomic(&d.join("variants/w.toml"), b"x").unwrap();
    write_atomic(&abs, b"y").unwrap();
    j.rollback().unwrap();
    assert_eq!(std::fs::read_to_string(d.join("variants/w.toml")).unwrap(), "nested");
    assert_eq!(std::fs::read_to_string(&abs).unwrap(), "absolute");
}

#[test]
fn a_live_editor_blocks_recovery_and_a_dead_one_does_not() {
    let d = project("live");
    std::fs::write(d.join("world.toml"), "x").unwrap();
    let j = Journal::begin(&d, &["world.toml"]).unwrap();
    // flock is per open file description: a second open in this process
    // contends like another process would.
    assert!(recover(&d).is_err());
    abandon(j);
    assert!(matches!(recover(&d).unwrap(), Recovery::Nothing));
}

#[test]
fn a_symlinked_manifest_is_written_through_its_link() {
    let d = project("link");
    std::fs::write(d.join("shared.toml"), "old").unwrap();
    std::os::unix::fs::symlink("shared.toml", d.join("world.toml")).unwrap();
    write_atomic(&d.join("world.toml"), b"new").unwrap();
    assert!(std::fs::symlink_metadata(d.join("world.toml")).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(d.join("shared.toml")).unwrap(), "new");
}

//! Edits to a project's manifest and lock that survive being interrupted
//! (T3b-b). `add`, `update` and `remove` write the new files, then compose to
//! check them; a failure put the old bytes back, but a Ctrl-C or a kill during
//! the compose left the edit in place with nothing to undo it. Now the old
//! bytes are saved first, in `.trantor-edit/`, and each file is replaced by
//! rename. A finished edit removes the journal; an interrupted one leaves it,
//! and the next trantor command in that directory puts the files back.
use std::path::{Path, PathBuf};

pub const JOURNAL_DIR: &str = ".trantor-edit";
const ABSENT: &str = ".absent";
const OWNER: &str = ".pid";

/// Replace `path` by renaming a sibling over it, keeping the old file's mode.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.trantor-tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&tmp, meta.permissions()).ok();
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        std::fs::remove_file(&tmp).ok();
        format!("write {}: {e}", path.display())
    })
}

pub struct Journal {
    dir: PathBuf,
    files: Vec<String>,
}

impl Journal {
    /// Save `files` (relative to `project`) before they are edited.
    pub fn begin(project: &Path, files: &[&str]) -> Result<Journal, String> {
        recover(project)?;
        let dir = project.join(JOURNAL_DIR);
        let staging = project.join(format!("{JOURNAL_DIR}.tmp-{}", std::process::id()));
        std::fs::remove_dir_all(&staging).ok();
        std::fs::create_dir_all(&staging).map_err(|e| format!("create {}: {e}", staging.display()))?;
        for f in files {
            let saved = match std::fs::read(project.join(f)) {
                Ok(bytes) => std::fs::write(staging.join(f), bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::write(staging.join(format!("{f}{ABSENT}")), ""),
                Err(e) => return Err(format!("read {}: {e}", project.join(f).display())),
            };
            saved.map_err(|e| format!("save {f} into {}: {e}", staging.display()))?;
        }
        std::fs::write(staging.join(OWNER), std::process::id().to_string()).map_err(|e| format!("write {}: {e}", staging.display()))?;
        std::fs::rename(&staging, &dir).map_err(|e| {
            std::fs::remove_dir_all(&staging).ok();
            format!("start the edit journal {}: {e}", dir.display())
        })?;
        Ok(Journal { dir, files: files.iter().map(|f| f.to_string()).collect() })
    }

    pub fn commit(self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }

    /// Put every file back as it was, then drop the journal.
    pub fn rollback(self) -> Result<(), String> {
        let project = self.dir.parent().unwrap_or(Path::new(".")).to_path_buf();
        restore(&project, &self.dir, &self.files)?;
        std::fs::remove_dir_all(&self.dir).map_err(|e| format!("remove {}: {e}", self.dir.display()))
    }
}

/// Undo an edit a killed trantor left half done. Returns what it restored.
pub fn recover(project: &Path) -> Result<Option<String>, String> {
    let dir = project.join(JOURNAL_DIR);
    if !dir.is_dir() {
        return Ok(None);
    }
    let owner = std::fs::read_to_string(dir.join(OWNER)).ok().and_then(|p| p.trim().parse::<u32>().ok());
    if let Some(pid) = owner.filter(|p| *p != std::process::id() && crate::bounded::alive(*p)) {
        return Err(format!("another trantor (pid {pid}) is editing {} — wait for it to finish", project.display()));
    }
    let files: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("read {}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != OWNER)
        .map(|n| n.strip_suffix(ABSENT).map(str::to_string).unwrap_or(n))
        .collect();
    restore(project, &dir, &files)?;
    std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    Ok(Some(format!(
        "an interrupted `trantor add`/`update`/`remove` in {} was undone: {} restored",
        project.display(),
        files.join(" and ")
    )))
}

/// Recover, telling the user when there was anything to recover.
pub fn recover_noting(project: &Path) -> Result<(), String> {
    if let Some(note) = recover(project)? {
        eprintln!("trantor: {note}");
    }
    Ok(())
}

fn restore(project: &Path, journal: &Path, files: &[String]) -> Result<(), String> {
    for f in files {
        let target = project.join(f);
        if journal.join(format!("{f}{ABSENT}")).exists() {
            if target.exists() {
                std::fs::remove_file(&target).map_err(|e| format!("restore {}: {e}", target.display()))?;
            }
            continue;
        }
        let saved = std::fs::read(journal.join(f)).map_err(|e| format!("read the saved {f}: {e}"))?;
        if std::fs::read(&target).ok().as_deref() != Some(saved.as_slice()) {
            write_atomic(&target, &saved).map_err(|e| format!("restore {}: {e}", target.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trantor-journal-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
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
        // A kill: nothing commits or rolls back, and the owner pid is gone.
        std::fs::write(j.dir.join(OWNER), "999999999").unwrap();
        std::mem::forget(j);
        let note = recover(&d).unwrap().expect("something to recover");
        assert!(note.contains("package.toml"), "{note}");
        assert_eq!(std::fs::read_to_string(d.join("package.toml")).unwrap(), "before");
        assert!(!d.join("trantor.lock").exists());
    }

    #[test]
    fn a_restore_that_changes_nothing_does_not_need_write_permission() {
        let d = project("readonly");
        let f = d.join("world.toml");
        std::fs::write(&f, "same").unwrap();
        let j = Journal::begin(&d, &["world.toml"]).unwrap();
        let mut perms = std::fs::metadata(&f).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&f, perms).unwrap();
        assert!(j.rollback().is_ok());
    }

    #[test]
    fn a_live_editor_is_not_rolled_back_under_it() {
        let d = project("live");
        std::fs::write(d.join("world.toml"), "x").unwrap();
        let j = Journal::begin(&d, &["world.toml"]).unwrap();
        std::fs::write(j.dir.join(OWNER), "1").unwrap(); // launchd: alive, never us
        assert!(recover(&d).unwrap_err().contains("pid 1"));
        std::fs::write(j.dir.join(OWNER), std::process::id().to_string()).unwrap();
        j.commit();
    }
}

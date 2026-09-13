//! Edits to a project's manifest and lock that survive being interrupted
//! (D-T3-12). `add`, `update` and `remove` save the old bytes in
//! `.trantor-edit/` before editing, record what they wrote once they have
//! written it, and then compose to check the result. A finished edit removes
//! the journal; an interrupted one leaves it for the next trantor command in
//! that directory.
//!
//! That command restores a file only if it still holds exactly what the edit
//! wrote. A file changed since — edited by hand, or by a `git pull` — is kept,
//! and the journal stays with a note saying so: recovery used to put the old
//! bytes back over whatever was there.
//!
//! The owner holds an flock on the journal for as long as it lives, so a live
//! editor is told apart from a dead one by the kernel, not by a pid that may
//! since belong to another process.
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

pub const JOURNAL_DIR: &str = ".trantor-edit";
const OWNER: &str = "owner.lock";
const PATHS: &str = "paths";

/// Replace `path` by renaming a sibling over it, keeping the old file's mode.
/// A symlinked manifest is written through its link, as a plain write was.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let path = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => path.canonicalize().map_err(|e| format!("resolve {}: {e}", path.display()))?,
        _ => path.to_path_buf(),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.trantor-tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(&path) {
        std::fs::set_permissions(&tmp, meta.permissions()).ok();
    }
    std::fs::rename(&tmp, &path).map_err(|e| {
        std::fs::remove_file(&tmp).ok();
        format!("write {}: {e}", path.display())
    })
}

pub struct Journal {
    dir: PathBuf,
    project: PathBuf,
    files: Vec<String>,
    /// Held for the journal's life; the kernel drops the lock if we die.
    _owner: File,
}

impl Journal {
    /// Save `files` (relative to `project`, or absolute) before they are edited.
    pub fn begin(project: &Path, files: &[&str]) -> Result<Journal, String> {
        if let Recovery::Conflict(why) = recover(project)? {
            return Err(why);
        }
        let dir = project.join(JOURNAL_DIR);
        let staging = Staging(project.join(format!("{JOURNAL_DIR}.tmp-{}", std::process::id())));
        std::fs::remove_dir_all(&staging.0).ok();
        std::fs::create_dir_all(&staging.0).map_err(|e| format!("create {}: {e}", staging.0.display()))?;
        let owner = File::create(staging.0.join(OWNER)).map_err(|e| format!("create the edit journal: {e}"))?;
        if !try_lock(&owner) {
            return Err("could not lock the edit journal".into());
        }
        std::fs::write(staging.0.join(PATHS), files.join("\n")).map_err(|e| format!("write the edit journal: {e}"))?;
        for (i, f) in files.iter().enumerate() {
            save(&staging.0, &format!("{i}.before"), &project.join(f))?;
        }
        std::fs::rename(&staging.0, &dir).map_err(|e| format!("start the edit journal {}: {e}", dir.display()))?;
        std::mem::forget(staging);
        Ok(Journal { dir, project: project.to_path_buf(), files: files.iter().map(|f| f.to_string()).collect(), _owner: owner })
    }

    /// Record what the edit wrote, so recovery can tell it from a later change.
    pub fn written(&self) -> Result<(), String> {
        for (i, f) in self.files.iter().enumerate() {
            save(&self.dir, &format!("{i}.after"), &self.project.join(f))?;
        }
        Ok(())
    }

    pub fn commit(self) {
        discard(&self.dir);
    }

    /// Put every file back as it was, then drop the journal.
    pub fn rollback(self) -> Result<(), String> {
        for (i, f) in self.files.iter().enumerate() {
            let before = saved(&self.dir, &format!("{i}.before"))?;
            restore(&self.project.join(f), &before)?;
        }
        discard(&self.dir);
        Ok(())
    }
}

pub enum Recovery {
    Nothing,
    Restored(String),
    /// The journal is kept, and why.
    Conflict(String),
}

/// Undo an edit a killed trantor left half done.
pub fn recover(project: &Path) -> Result<Recovery, String> {
    sweep(project);
    let dir = project.join(JOURNAL_DIR);
    if !dir.is_dir() {
        return Ok(Recovery::Nothing);
    }
    let owner = File::open(dir.join(OWNER)).ok();
    if owner.as_ref().is_some_and(|o| !try_lock(o)) {
        return Err(format!("another trantor is editing {} — wait for it to finish", project.display()));
    }
    let files: Vec<String> = std::fs::read_to_string(dir.join(PATHS)).unwrap_or_default().lines().map(str::to_string).collect();
    let (mut restore_these, mut changed) = (vec![], vec![]);
    for (i, f) in files.iter().enumerate() {
        let target = project.join(f);
        let current = current(&target)?;
        let before = saved(&dir, &format!("{i}.before"))?;
        if current == before {
            continue;
        }
        match saved(&dir, &format!("{i}.after")) {
            Ok(after) if after == current => restore_these.push((target, before)),
            _ => changed.push(f.clone()),
        }
    }
    if !changed.is_empty() {
        return Ok(Recovery::Conflict(format!(
            "an interrupted `trantor add`/`update`/`remove` left {}, but {} changed since, so nothing was restored. \
             Delete {} to keep the files as they are, or copy back the saved `*.before` files from it",
            dir.display(),
            changed.join(" and "),
            dir.display()
        )));
    }
    let names: Vec<String> = restore_these.iter().map(|(t, _)| t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect();
    for (target, before) in &restore_these {
        restore(target, before)?;
    }
    discard(&dir);
    Ok(if names.is_empty() {
        Recovery::Nothing
    } else {
        Recovery::Restored(format!("an interrupted `trantor add`/`update`/`remove` in {} was undone: {} restored", project.display(), names.join(" and ")))
    })
}

/// Recover before a command that does not edit; a conflict is reported and
/// the command goes ahead on the files as they are.
pub fn recover_noting(project: &Path) -> Result<(), String> {
    match recover(project)? {
        Recovery::Nothing => {}
        Recovery::Restored(note) | Recovery::Conflict(note) => eprintln!("trantor: {note}"),
    }
    Ok(())
}

pub fn try_lock(f: &File) -> bool {
    // SAFETY: flock on an fd we own; no memory involved.
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

/// A file's bytes, or None when it does not exist.
fn current(p: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(p) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("read {}: {e}", p.display())),
    }
}

fn save(journal: &Path, key: &str, from: &Path) -> Result<(), String> {
    let (name, bytes) = match current(from)? {
        Some(b) => (key.to_string(), b),
        None => (format!("{key}.absent"), vec![]),
    };
    std::fs::remove_file(journal.join(format!("{key}.absent"))).ok();
    write_atomic(&journal.join(name), &bytes)
}

fn saved(journal: &Path, key: &str) -> Result<Option<Vec<u8>>, String> {
    if journal.join(format!("{key}.absent")).exists() {
        return Ok(None);
    }
    std::fs::read(journal.join(key)).map(Some).map_err(|e| format!("read the saved {key}: {e}"))
}

fn restore(target: &Path, bytes: &Option<Vec<u8>>) -> Result<(), String> {
    match bytes {
        None if target.exists() => std::fs::remove_file(target).map_err(|e| format!("restore {}: {e}", target.display())),
        None => Ok(()),
        Some(b) if current(target)?.as_deref() == Some(b.as_slice()) => Ok(()),
        Some(b) => write_atomic(target, b).map_err(|e| format!("restore {}: {e}", target.display())),
    }
}

/// Renamed away first, so a kill mid-delete never leaves a partial journal
/// that looks like an interrupted edit.
fn discard(dir: &Path) {
    let gone = dir.with_file_name(format!("{JOURNAL_DIR}.done-{}", std::process::id()));
    if std::fs::rename(dir, &gone).is_ok() {
        std::fs::remove_dir_all(&gone).ok();
    }
}

/// Staging and discarded journals whose owner is gone.
fn sweep(project: &Path) {
    let Ok(entries) = std::fs::read_dir(project) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let stale = name.starts_with(&format!("{JOURNAL_DIR}.tmp-")) || name.starts_with(&format!("{JOURNAL_DIR}.done-"));
        if stale && File::open(e.path().join(OWNER)).map_or(true, |o| try_lock(&o)) {
            std::fs::remove_dir_all(e.path()).ok();
        }
    }
}

/// A staging directory removed unless it became the journal.
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;

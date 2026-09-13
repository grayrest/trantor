//! The marker a running `trantor new` keeps in its directory (T3d). A `new`
//! killed part way — a Ctrl-C during a long build — left a half project that
//! the next `new` refused. The marker lets the next one clean up, and only
//! what is provably the killed run's: a file it wrote that still holds exactly
//! those bytes, a directory it created that is empty, and `target/`, which is
//! all generated. Anything changed since is kept, and `new` refuses naming it —
//! a cleanup that removed everything the killed run listed destroyed files the
//! user had added in between.
//!
//! The marker is claimed by linking an already-locked file into place, so it
//! exists only while locked: a second `new` racing the first cannot take it,
//! and cannot mistake a live run for a killed one.
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

const MARKER: &str = ".trantor-new";
/// Generated in full, so removed whole.
const GENERATED: &str = "target";

pub struct Marker {
    dir: PathBuf,
    file: File,
}

impl Marker {
    /// Claim `dir` for a `new`, after cleaning up a killed one.
    pub fn claim(dir: &Path) -> Result<Marker, String> {
        let mut missing: Vec<PathBuf> = dir.ancestors().take_while(|a| !a.as_os_str().is_empty() && !a.exists()).map(Path::to_path_buf).collect();
        missing.reverse();
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let path = dir.join(MARKER);
        let staging = dir.join(format!("{MARKER}.tmp-{}", std::process::id()));
        let mut file = File::create(&staging).map_err(|e| format!("create {}: {e}", staging.display()))?;
        if !crate::journal::try_lock(&file) {
            return Err(format!("could not lock {}", staging.display()));
        }
        for d in &missing {
            writeln!(file, "dir\t{}", d.display()).map_err(|e| format!("write {}: {e}", staging.display()))?;
        }
        let linked = match std::fs::hard_link(&staging, &path) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => undo_killed(dir)
                .and_then(|()| std::fs::hard_link(&staging, &path).map_err(|e| format!("another `trantor new` is starting in {} ({e})", dir.display()))),
            other => other.map_err(|e| format!("create {}: {e}", path.display())),
        };
        std::fs::remove_file(&staging).ok();
        if let Err(e) = linked {
            for d in missing.iter().rev() {
                std::fs::remove_dir(d).ok();
            }
            return Err(e);
        }
        Ok(Marker { dir: dir.to_path_buf(), file })
    }

    /// `rel` (under the directory) was just written by this `new`.
    pub fn wrote(&mut self, rel: &str) -> Result<(), String> {
        let bytes = std::fs::read(self.dir.join(rel)).map_err(|e| format!("read {rel}: {e}"))?;
        writeln!(self.file, "file\t{rel}\t{:016x}", fnv(&bytes)).map_err(|e| format!("write the new marker: {e}"))
    }

    /// `rel` (under the directory) is about to be created by this `new`.
    pub fn creating_dir(&mut self, rel: &str) -> Result<(), String> {
        if self.dir.join(rel).exists() {
            return Ok(());
        }
        writeln!(self.file, "dir\t{}", self.dir.join(rel).display()).map_err(|e| format!("write the new marker: {e}"))
    }

    /// Success: the project is the user's now.
    pub fn finish(self) {
        std::fs::remove_file(self.dir.join(MARKER)).ok();
    }

    /// Failure: remove what this run wrote, as a killed run's would be.
    pub fn undo(self) -> Result<(), String> {
        undo_entries(&self.dir)
    }
}

/// A marker whose lock can be taken belongs to a killed `new`.
fn undo_killed(dir: &Path) -> Result<(), String> {
    let marker = File::open(dir.join(MARKER)).map_err(|e| format!("open {}: {e}", dir.join(MARKER).display()))?;
    if !crate::journal::try_lock(&marker) {
        return Err(format!("another `trantor new` is running in {}", dir.display()));
    }
    undo_entries(dir)?;
    eprintln!("trantor: an interrupted `trantor new` in {} was cleaned up", dir.display());
    Ok(())
}

fn undo_entries(dir: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(dir.join(MARKER)).unwrap_or_default();
    let mut files: Vec<(String, String)> = vec![];
    let mut dirs: Vec<PathBuf> = vec![];
    for line in text.lines() {
        match line.split('\t').collect::<Vec<_>>().as_slice() {
            ["file", rel, hash] => {
                files.retain(|(r, _)| r != rel);
                files.push((rel.to_string(), hash.to_string()));
            }
            ["dir", path] => dirs.push(PathBuf::from(path)),
            _ => {}
        }
    }
    let mut changed = vec![];
    for (rel, hash) in &files {
        let path = dir.join(rel);
        match std::fs::read(&path) {
            Ok(bytes) if format!("{:016x}", fnv(&bytes)) == *hash => { std::fs::remove_file(&path).ok(); }
            Ok(_) => changed.push(rel.clone()),
            Err(_) => {}
        }
    }
    for d in dirs.iter().rev() {
        if d.file_name().is_some_and(|n| n == GENERATED) && d.parent() == Some(dir) {
            std::fs::remove_dir_all(d).ok();
        }
    }
    if !changed.is_empty() {
        return Err(format!(
            "an interrupted `trantor new` in {} wrote {}, which changed since, so they were kept. \
             Delete them to start over, or delete {} to keep the project as it is",
            dir.display(),
            changed.join(", "),
            dir.join(MARKER).display()
        ));
    }
    std::fs::remove_file(dir.join(MARKER)).ok();
    for d in dirs.iter().rev() {
        std::fs::remove_dir(d).ok(); // only if empty
    }
    Ok(())
}

/// FNV-1a: stable across builds, which a std hasher is not promised to be.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trantor-marker-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn a_killed_new_is_cleaned_up_but_files_changed_since_are_kept() {
        let d = tmp("killed").join("proj");
        let mut m = Marker::claim(&d).unwrap();
        std::fs::write(d.join("world.toml"), "[world]").unwrap();
        m.wrote("world.toml").unwrap();
        std::fs::write(d.join("Cargo.toml"), "[workspace]").unwrap();
        m.wrote("Cargo.toml").unwrap();
        drop(m); // killed: the lock goes, the marker stays
        std::fs::write(d.join("Cargo.toml"), "[workspace] # mine").unwrap();
        std::fs::write(d.join("NOTES.md"), "mine").unwrap();
        let e = Marker::claim(&d).err().expect("a changed file refuses");
        assert!(e.contains("Cargo.toml"), "{e}");
        assert!(!d.join("world.toml").exists(), "an untouched file is removed");
        assert!(d.join("Cargo.toml").exists() && d.join("NOTES.md").exists(), "the user's files stay");
    }

    #[test]
    fn a_live_new_is_not_taken_for_a_killed_one() {
        let d = tmp("live");
        let _m = Marker::claim(&d).unwrap();
        let e = Marker::claim(&d).err().expect("second claim refused");
        assert!(e.contains("another `trantor new`"), "{e}");
    }

    #[test]
    fn an_undone_new_removes_the_directories_it_created_when_empty() {
        let base = tmp("dirs");
        let d = base.join("x/y/z");
        let mut m = Marker::claim(&d).unwrap();
        m.creating_dir("target").unwrap();
        std::fs::create_dir_all(d.join("target/trantor")).unwrap();
        std::fs::write(d.join("world.toml"), "w").unwrap();
        m.wrote("world.toml").unwrap();
        m.undo().unwrap();
        assert!(!base.join("x").exists(), "every directory new created is gone");
    }
}

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
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

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
        let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut missing: Vec<PathBuf> = dir.ancestors().take_while(|a| !a.exists()).map(Path::to_path_buf).collect();
        missing.reverse();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        // Resolved, so a rerun through another spelling (`/tmp`, `..`) matches.
        let resolve = |p: &Path| p.canonicalize().map_err(|e| format!("{}: {e}", p.display()));
        let dir = resolve(&dir)?;
        let missing = missing.iter().map(|m| resolve(m)).collect::<Result<Vec<_>, _>>()?;
        let mut header = format!("project\t{}\n", dir.display());
        for d in &missing {
            header.push_str(&format!("made\t{}\n", d.display()));
        }
        match place(&dir, &header) {
            Ok(file) => Ok(Marker { dir, file }),
            Err(e) => {
                for d in missing.iter().rev() {
                    std::fs::remove_dir(d).ok();
                }
                Err(e)
            }
        }
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
        writeln!(self.file, "sub\t{rel}").map_err(|e| format!("write the new marker: {e}"))
    }

    /// Success: the project is the user's now.
    pub fn finish(self) {
        remove_if_same(&self.dir.join(MARKER), &self.file);
    }

    /// Failure: remove what this run wrote, as a killed run's would be.
    pub fn undo(mut self) -> Result<(), String> {
        undo_entries(&self.dir, &mut self.file)
    }
}

/// Put a locked marker holding `header` in place, cleaning up a killed run's
/// first. Linking an already-locked file means the marker is never visible
/// unlocked; where hard links are not supported, it is created in place.
fn place(dir: &Path, header: &str) -> Result<File, String> {
    let path = dir.join(MARKER);
    let staging = dir.join(format!("{MARKER}.tmp-{}", std::process::id()));
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&staging)
        .map_err(|e| format!("create {}: {e}", staging.display()))?;
    if !crate::journal::try_lock(&file) {
        return Err(format!("could not lock {}", staging.display()));
    }
    file.write_all(header.as_bytes()).map_err(|e| format!("write {}: {e}", staging.display()))?;
    let mut linked = std::fs::hard_link(&staging, &path);
    if linked.as_ref().is_err_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) {
        undo_killed(dir)?;
        linked = std::fs::hard_link(&staging, &path);
    }
    std::fs::remove_file(&staging).ok();
    match linked {
        Ok(()) => Ok(file),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!("another `trantor new` is starting in {}", dir.display())),
        Err(_) => {
            let mut direct = OpenOptions::new().read(true).write(true).create_new(true).open(&path)
                .map_err(|e| format!("create {}: {e}", path.display()))?;
            crate::journal::try_lock(&direct);
            direct.write_all(header.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
            Ok(direct)
        }
    }
}

/// A marker whose lock can be taken belongs to a killed `new`.
fn undo_killed(dir: &Path) -> Result<(), String> {
    let mut marker = File::open(dir.join(MARKER)).map_err(|e| format!("open {}: {e}", dir.join(MARKER).display()))?;
    if !crate::journal::try_lock(&marker) {
        return Err(format!("another `trantor new` is running in {}", dir.display()));
    }
    undo_entries(dir, &mut marker)?;
    eprintln!("trantor: an interrupted `trantor new` in {} was cleaned up", dir.display());
    Ok(())
}

/// A relative path that stays under the directory: no `..`, no root.
fn inside(rel: &str) -> bool {
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
}

/// Undo what the marker behind `handle` lists. The marker is read through the
/// handle that holds its lock, not by path — another run may have replaced the
/// file at that path by now — and only paths inside the project, or the
/// directories `new` made above it, are touched.
fn undo_entries(dir: &Path, handle: &mut File) -> Result<(), String> {
    let mut text = String::new();
    handle.rewind().and_then(|()| handle.read_to_string(&mut text)).map_err(|e| format!("read the new marker: {e}"))?;
    let refuse = |why: &str| format!("{} {why}; delete it if no `trantor new` is running there", dir.join(MARKER).display());
    let (mut files, mut subs, mut made): (Vec<(String, String)>, Vec<String>, Vec<PathBuf>) = (vec![], vec![], vec![]);
    let mut project = None;
    for line in text.lines() {
        match line.split('\t').collect::<Vec<_>>().as_slice() {
            ["project", p] => project = Some(PathBuf::from(p)),
            ["file", rel, hash] if inside(rel) => {
                files.retain(|(r, _)| r != rel);
                files.push((rel.to_string(), hash.to_string()));
            }
            ["sub", rel] if inside(rel) => subs.push(rel.to_string()),
            ["made", p] if dir.starts_with(p) => made.push(PathBuf::from(p)),
            _ => return Err(refuse("holds an entry trantor did not write")),
        }
    }
    if project.as_deref() != Some(dir) {
        return Err(refuse("was written for another directory (moved, or cloned)"));
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
    for rel in subs.iter().rev() {
        if rel == GENERATED {
            std::fs::remove_dir_all(dir.join(rel)).ok();
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
    remove_if_same(&dir.join(MARKER), handle);
    for rel in subs.iter().rev() {
        std::fs::remove_dir(dir.join(rel)).ok(); // only if empty
    }
    for d in made.iter().rev() {
        std::fs::remove_dir(d).ok(); // only if empty
    }
    Ok(())
}

/// Remove the marker at `path` only if it is still the file `handle` opened.
fn remove_if_same(path: &Path, handle: &File) {
    let same = std::fs::metadata(path).ok().zip(handle.metadata().ok()).is_some_and(|(a, b)| a.ino() == b.ino() && a.dev() == b.dev());
    if same {
        std::fs::remove_file(path).ok();
    }
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
    fn a_marker_cannot_reach_outside_its_project_or_follow_it_elsewhere() {
        let base = tmp("evil");
        let victim = base.join("victim.txt");
        std::fs::create_dir_all(base.join("proj")).unwrap();
        std::fs::write(&victim, "keep").unwrap();
        let hash = format!("{:016x}", fnv(b"keep"));
        let proj = base.join("proj");
        std::fs::write(proj.join(MARKER), format!("project\t{}\nfile\t../victim.txt\t{hash}\n", proj.display())).unwrap();
        assert!(Marker::claim(&proj).is_err());
        assert!(victim.exists(), "a path out of the project is refused, not followed");
        std::fs::write(proj.join(MARKER), "project\t/somewhere/else\n").unwrap();
        let e = Marker::claim(&proj).err().expect("another directory's marker");
        assert!(e.contains("another directory"), "{e}");
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

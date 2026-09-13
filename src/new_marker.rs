//! The marker a running `trantor new` keeps in its directory (D-T3-22). It
//! records what that `new` created, and its owner holds an flock on it.
//!
//! A `new` that fails removes what it wrote, while it is still running and
//! knows what that is. A `new` killed part way leaves its marker, and the next
//! `new` in that directory refuses, listing what the killed run wrote and
//! whether each file is unchanged — it deletes nothing. Automatic cleanup after
//! a kill was tried and kept finding ways to delete what it should not (through
//! a symlink, a rerun race, an unnormalised path), and a leftover half project
//! is the user's to judge (user, 2026-09-13).
//!
//! The marker is placed by linking an already-locked file, so it is never
//! visible unlocked: a second `new` cannot take a live run for a killed one.
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

const MARKER: &str = ".trantor-new";
/// Generated in full, so an undone run removes it whole.
const GENERATED: &str = "target";

pub struct Marker {
    dir: PathBuf,
    file: File,
}

impl Marker {
    /// Claim `dir` for a `new`. A marker already there is a running `new` or a
    /// killed one; either way this one does not start.
    pub fn claim(dir: &Path) -> Result<Marker, String> {
        if dir.as_os_str().to_string_lossy().chars().any(char::is_control) {
            return Err(format!("new: {:?} has a control character in it", dir.display()));
        }
        let dir = normalise(dir)?;
        let mut missing: Vec<PathBuf> = dir.ancestors().take_while(|a| !a.exists()).map(Path::to_path_buf).collect();
        missing.reverse();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        // Resolved, so the entries name what is on disk (`/tmp` is `/private/tmp`).
        let resolve = |p: &Path| p.canonicalize().map_err(|e| format!("{}: {e}", p.display()));
        let (dir, missing) = (resolve(&dir)?, missing.iter().map(|m| resolve(m)).collect::<Result<Vec<_>, _>>()?);
        let mut header = format!("project\t{}\n", dir.display());
        for d in &missing {
            header.push_str(&format!("made\t{}\n", d.display()));
        }
        match place(&dir, &header) {
            Ok(file) => Ok(Marker { dir, file }),
            Err(e) => {
                for d in missing.iter().rev() {
                    std::fs::remove_dir(d).ok(); // only if empty
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

    /// Failure, in this same run: remove what it wrote.
    pub fn undo(mut self) -> Result<(), String> {
        let mut text = String::new();
        self.file.rewind().and_then(|()| self.file.read_to_string(&mut text)).map_err(|e| format!("read the new marker: {e}"))?;
        let entries = Entries::parse(&text).filter(|e| e.project.as_deref() == Some(self.dir.as_path()));
        let Some(entries) = entries else {
            return Err(format!("{} is not as this `new` wrote it; nothing was removed", self.dir.join(MARKER).display()));
        };
        let mut kept = vec![];
        for (rel, hash) in &entries.files {
            let path = self.dir.join(rel);
            match std::fs::read(&path) {
                Ok(bytes) if format!("{:016x}", fnv(&bytes)) == *hash && no_symlink_under(&self.dir, rel) => { std::fs::remove_file(&path).ok(); }
                Ok(_) => kept.push(rel.clone()),
                Err(_) => {}
            }
        }
        for rel in entries.subs.iter().rev() {
            if !no_symlink_under(&self.dir, rel) {
                continue;
            }
            if rel == GENERATED {
                std::fs::remove_dir_all(self.dir.join(rel)).ok();
            } else {
                std::fs::remove_dir(self.dir.join(rel)).ok(); // only if empty
            }
        }
        if !kept.is_empty() {
            return Err(format!("{} changed while this `new` ran and were kept", kept.join(", ")));
        }
        remove_if_same(&self.dir.join(MARKER), &self.file);
        for d in entries.made.iter().rev().filter(|d| self.dir.starts_with(d)) {
            std::fs::remove_dir(d).ok(); // only if empty
        }
        Ok(())
    }
}

/// What a marker lists.
struct Entries {
    project: Option<PathBuf>,
    files: Vec<(String, String)>,
    subs: Vec<String>,
    made: Vec<PathBuf>,
}

impl Entries {
    /// `None` for a marker holding anything this trantor does not write.
    fn parse(text: &str) -> Option<Entries> {
        let mut e = Entries { project: None, files: vec![], subs: vec![], made: vec![] };
        for line in text.lines() {
            match line.split('\t').collect::<Vec<_>>().as_slice() {
                ["project", p] => e.project = Some(PathBuf::from(p)),
                ["file", rel, hash] if inside(rel) => {
                    e.files.retain(|(r, _)| r != rel);
                    e.files.push((rel.to_string(), hash.to_string()));
                }
                ["sub", rel] if inside(rel) => e.subs.push(rel.to_string()),
                ["made", p] => e.made.push(PathBuf::from(p)),
                _ => return None,
            }
        }
        Some(e)
    }
}

/// Place a locked marker holding `header`. Linking an already-locked file
/// means the marker is never visible unlocked; where hard links are not
/// supported it is created in place and locked, and refused if that fails.
fn place(dir: &Path, header: &str) -> Result<File, String> {
    let path = dir.join(MARKER);
    sweep_staging(dir);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let staging = dir.join(format!("{MARKER}.tmp-{}-{nanos}", std::process::id()));
    let mut file = OpenOptions::new().read(true).write(true).create_new(true).open(&staging)
        .map_err(|e| format!("create {}: {e}", staging.display()))?;
    let locked = crate::journal::try_lock(&file) && file.write_all(header.as_bytes()).is_ok();
    let linked = if locked { std::fs::hard_link(&staging, &path) } else { Err(std::io::Error::other("could not lock the new marker")) };
    std::fs::remove_file(&staging).ok();
    match linked {
        Ok(()) => Ok(file),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(explain_existing(dir)),
        Err(_) if locked => {
            let mut direct = match OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(explain_existing(dir)),
                Err(e) => return Err(format!("create {}: {e}", path.display())),
            };
            if !crate::journal::try_lock(&direct) || direct.write_all(header.as_bytes()).is_err() {
                return Err(format!("another `trantor new` is starting in {}", dir.display()));
            }
            Ok(direct)
        }
        Err(e) => Err(format!("{e} in {}", dir.display())),
    }
}

/// Why a directory with a marker cannot get a new `new`: another is running,
/// or one was killed, and here is what it left.
fn explain_existing(dir: &Path) -> String {
    let path = dir.join(MARKER);
    let Ok(mut marker) = File::open(&path) else {
        return format!("another `trantor new` is starting in {}", dir.display());
    };
    if !crate::journal::try_lock(&marker) {
        return format!("another `trantor new` is running in {}", dir.display());
    }
    let mut text = String::new();
    marker.read_to_string(&mut text).ok();
    let mut left = vec![];
    if let Some(e) = Entries::parse(&text) {
        for (rel, hash) in &e.files {
            let state = match std::fs::read(dir.join(rel)) {
                Ok(bytes) if format!("{:016x}", fnv(&bytes)) == *hash => "as it wrote it",
                Ok(_) => "changed since",
                Err(_) => "gone",
            };
            left.push(format!("{rel} ({state})"));
        }
        left.extend(e.subs.iter().filter(|s| dir.join(s).exists()).map(|s| format!("{s}/")));
    }
    let listed = if left.is_empty() { String::from("nothing it recorded") } else { left.join(", ") };
    format!(
        "an interrupted `trantor new` left {} — it wrote {listed}. Delete what you do not want to keep, \
         and the marker, then run `trantor new` again; trantor removes nothing after a kill",
        path.display()
    )
}

/// Leftover staging files of killed runs: nothing holds their lock.
fn sweep_staging(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let stale = e.file_name().to_string_lossy().starts_with(&format!("{MARKER}.tmp-"))
            && File::open(e.path()).is_ok_and(|f| crate::journal::try_lock(&f));
        if stale {
            std::fs::remove_file(e.path()).ok();
        }
    }
}

/// `dir` made absolute with `.` and `..` folded, so its missing ancestors are
/// the ones that will be created.
fn normalise(dir: &Path) -> Result<PathBuf, String> {
    let abs = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => { out.pop(); }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    Ok(out)
}

/// A relative path that stays under the directory: no `..`, no root.
fn inside(rel: &str) -> bool {
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
}

/// No component of `rel` under `dir` is a symlink, so removing it stays inside.
fn no_symlink_under(dir: &Path, rel: &str) -> bool {
    let mut at = dir.to_path_buf();
    Path::new(rel).components().all(|c| {
        at.push(c);
        std::fs::symlink_metadata(&at).map_or(true, |m| !m.file_type().is_symlink())
    })
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
    fn a_killed_new_is_explained_and_nothing_is_removed() {
        let d = tmp("killed").join("proj");
        let mut m = Marker::claim(&d).unwrap();
        std::fs::write(d.join("world.toml"), "[world]").unwrap();
        m.wrote("world.toml").unwrap();
        std::fs::write(d.join("Cargo.toml"), "[workspace]").unwrap();
        m.wrote("Cargo.toml").unwrap();
        drop(m); // killed: the lock goes, the marker stays
        std::fs::write(d.join("Cargo.toml"), "[workspace] # mine").unwrap();
        let e = Marker::claim(&d).err().expect("a killed run's marker refuses");
        assert!(e.contains("world.toml (as it wrote it)") && e.contains("Cargo.toml (changed since)"), "{e}");
        assert!(d.join("world.toml").exists() && d.join("Cargo.toml").exists() && d.join(MARKER).exists(), "nothing removed");
    }

    #[test]
    fn a_live_new_is_not_taken_for_a_killed_one() {
        let d = tmp("live");
        let _m = Marker::claim(&d).unwrap();
        let e = Marker::claim(&d).err().expect("second claim refused");
        assert!(e.contains("another `trantor new` is running"), "{e}");
    }

    #[test]
    fn a_foreign_or_hostile_marker_is_explained_and_nothing_outside_is_touched() {
        let base = tmp("evil");
        let proj = base.join("proj");
        std::fs::create_dir_all(base.join("outside")).unwrap();
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(base.join("outside/victim.txt"), "keep").unwrap();
        std::os::unix::fs::symlink("../outside", proj.join("link")).unwrap();
        let hash = format!("{:016x}", fnv(b"keep"));
        std::fs::write(proj.join(MARKER), format!("project\t{}\nfile\tlink/victim.txt\t{hash}\nfile\t../outside/victim.txt\t{hash}\n", proj.canonicalize().unwrap().display())).unwrap();
        assert!(Marker::claim(&proj).is_err());
        assert!(base.join("outside/victim.txt").exists());
    }

    #[test]
    fn a_failing_run_undoes_its_own_files_and_directories() {
        let base = tmp("dirs");
        let mut m = Marker::claim(&base.join("x/y/../z")).unwrap();
        let d = base.join("x/z");
        m.creating_dir("target").unwrap();
        std::fs::create_dir_all(d.join("target/trantor")).unwrap();
        std::fs::write(d.join("world.toml"), "w").unwrap();
        m.wrote("world.toml").unwrap();
        m.undo().unwrap();
        assert!(!base.join("x").exists(), "every directory new created is gone, `..` folded");
    }
}

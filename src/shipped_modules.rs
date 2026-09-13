//! Which component put each `platform/<Module>.roc` there. Two sources for one
//! module used to be settled silently by write order: the later copy won, so a
//! dev-dependency's `Helper` could replace a package's own and its failing
//! expects never ran (T3b-2). One module, one source — anything else is an
//! error naming both.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Shipped(RefCell<BTreeMap<String, (PathBuf, String)>>);

impl Shipped {
    /// Claim `module` for the file `from`, shipped by `who`.
    pub fn claim(&self, module: &str, from: &Path, who: &str) -> Result<(), String> {
        let mut seen = self.0.borrow_mut();
        match seen.get(module) {
            Some((path, _)) if path == from => Ok(()),
            Some((path, first)) => Err(format!(
                "two sources for the platform module `{module}`: {first} ({}) and {who} ({}). \
                 A module name can come from one place only — rename one, or leave one out of the world",
                path.display(),
                from.display()
            )),
            None => {
                seen.insert(module.to_string(), (from.to_path_buf(), who.to_string()));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_source_for_one_module_is_an_error_naming_both() {
        let s = Shipped::default();
        s.claim("Helper", Path::new("/pkg/Helper.roc"), "component `greet-lib`").unwrap();
        let e = s.claim("Helper", Path::new("/dev/Helper.roc"), "component `zz-helper`").unwrap_err();
        assert!(e.contains("greet-lib") && e.contains("zz-helper"), "{e}");
    }

    #[test]
    fn the_same_file_claimed_twice_is_one_source() {
        let s = Shipped::default();
        s.claim("Fs", Path::new("/i/Fs.roc"), "interface `a`").unwrap();
        assert!(s.claim("Fs", Path::new("/i/Fs.roc"), "interface `b`").is_ok());
    }
}

use anyhow::{anyhow, Result};
use std::path::{Component, Path, PathBuf};

#[derive(Clone)]
pub struct Jail {
    roots: Vec<PathBuf>,
}

impl Jail {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let roots = roots.into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect();
        Self { roots }
    }

    pub fn add_root(&mut self, root: PathBuf) {
        let canon = root.canonicalize().unwrap_or(root);
        if !self.roots.contains(&canon) { self.roots.push(canon); }
    }

    /// Resolve `requested` (workspace-relative — no `..`, no leading `/`, no `~`) against
    /// the first root that contains it. Returns an absolute canonical path.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf> {
        let p = Path::new(requested);
        for comp in p.components() {
            match comp {
                Component::ParentDir => return Err(anyhow!("path component '..' is forbidden")),
                Component::RootDir => return Err(anyhow!("absolute paths are forbidden")),
                Component::Prefix(_) => return Err(anyhow!("path prefix is forbidden")),
                _ => {}
            }
        }
        if requested.starts_with('~') { return Err(anyhow!("'~' expansion is forbidden")); }

        // Try each root in order; resolve and check containment.
        for root in &self.roots {
            let candidate = root.join(p);
            // If the candidate exists, canonicalize the WHOLE thing so a symlink leaf
            // pointing outside the root is caught. If it doesn't exist (typical for
            // writes), fall back to canonicalizing the parent.
            let canon_for_check: PathBuf = match candidate.canonicalize() {
                Ok(c) => c,
                Err(_) => candidate.parent()
                    .map(|pp| pp.canonicalize().unwrap_or_else(|_| pp.to_path_buf()))
                    .unwrap_or_else(|| root.clone()),
            };
            if canon_for_check.starts_with(root) {
                return Ok(candidate);
            }
        }
        Err(anyhow!("path '{requested}' escapes all jail roots"))
    }

    pub fn validate_inside_any(&self, abs: &Path) -> Result<()> {
        // Same structural guards as resolve() — reject any '..' or weird components
        // even if the path doesn't exist yet (canonicalize would silently fall through).
        for comp in abs.components() {
            if matches!(comp, Component::ParentDir) {
                return Err(anyhow!("path '{}' contains forbidden '..' component", abs.display()));
            }
        }
        let canon = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
        for root in &self.roots {
            if canon.starts_with(root) { return Ok(()); }
        }
        Err(anyhow!("path '{}' is outside jail", abs.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn forbids_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        assert!(jail.resolve("../etc/passwd").is_err());
    }

    #[test]
    fn forbids_absolute() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        assert!(jail.resolve("/etc/passwd").is_err());
    }

    #[test]
    fn resolves_simple_relative() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        let p = jail.resolve("foo/bar.txt").unwrap();
        // Canonicalize tmp.path() so the comparison survives macOS /var→/private/var symlinks.
        let canon_root = tmp.path().canonicalize().unwrap_or_else(|_| tmp.path().to_path_buf());
        assert!(p.starts_with(&canon_root));
        assert!(p.ends_with("foo/bar.txt"));
    }

    #[test]
    fn rejects_symlink_leaf_pointing_outside_root() {
        let jail_root = TempDir::new().unwrap();
        let outside_root = TempDir::new().unwrap();
        let outside_target = outside_root.path().join("secret.txt");
        std::fs::write(&outside_target, "leaked").unwrap();

        // Create a symlink inside the jail that points to a file outside the jail.
        let link_path = jail_root.path().join("escape");
        std::os::unix::fs::symlink(&outside_target, &link_path).unwrap();

        let jail = Jail::new(vec![jail_root.path().to_path_buf()]);
        // resolve must NOT return a path that the caller could read to leak the outside file.
        assert!(jail.resolve("escape").is_err(),
            "resolve allowed a symlink leaf to escape the jail");
    }

    #[test]
    fn validate_inside_any_rejects_parent_dir_in_nonexistent_path() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        let abs = tmp.path().join("a/../../../etc/passwd");
        // Path doesn't exist; canonicalize fails; old code accepted via literal starts_with.
        assert!(jail.validate_inside_any(&abs).is_err(),
            "validate_inside_any allowed a '..' component in a non-existent path");
    }
}

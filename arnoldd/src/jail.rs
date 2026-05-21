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
            // Canonicalize parent so symlink escapes are caught.
            let parent_canon = candidate.parent()
                .map(|pp| pp.canonicalize().unwrap_or_else(|_| pp.to_path_buf()))
                .unwrap_or_else(|| root.clone());
            if parent_canon.starts_with(root) {
                return Ok(candidate);
            }
        }
        Err(anyhow!("path '{requested}' escapes all jail roots"))
    }

    pub fn validate_inside_any(&self, abs: &Path) -> Result<()> {
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
}

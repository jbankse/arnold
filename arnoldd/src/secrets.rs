use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Per-provider API keys. Lives in ~/.arnold/secrets.toml (chmod 600). Loaded
/// at daemon startup; mutable via ClientRequest::SetProviderKey from the TUI.
///
/// Storage shape is flat strings keyed by short provider names. Empty values
/// are valid and mean "not set" — they're filtered out when exporting to env.
/// New providers can be added without a schema change because we keep extras
/// in a BTreeMap (via #[serde(flatten)]).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub anthropic_api_key: String,
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default)]
    pub xai_api_key: String,
    /// Catch-all for providers not enumerated above. Format: `<provider>_api_key = "..."`.
    #[serde(flatten)]
    pub extras: BTreeMap<String, String>,
}

impl Secrets {
    /// Load from `path`. If the file doesn't exist, returns an empty Secrets.
    /// Malformed TOML returns Err — we don't want to silently lose existing keys.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading secrets file {}", path.display()))?;
        let s: Self = toml::from_str(&text)
            .with_context(|| format!("parsing secrets file {}", path.display()))?;
        Ok(s)
    }

    /// Write the secrets file atomically with chmod 600. Best-effort permission
    /// set — on file systems that don't support unix modes this is a no-op.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string(self).context("serializing secrets")?;
        let tmp = path.with_extension("toml.new");
        std::fs::write(&tmp, &text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Default path: `~/.arnold/secrets.toml`. Centralized so tests can use
    /// tempdirs without duplicating the convention.
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("no home dir")?;
        Ok(home.join(".arnold/secrets.toml"))
    }

    /// Update one provider's key. `provider` is the short name ("anthropic",
    /// "openai", "xai", or arbitrary). Empty `key` clears it.
    pub fn set(&mut self, provider: &str, key: String) {
        match provider {
            "anthropic" => self.anthropic_api_key = key,
            "openai" => self.openai_api_key = key,
            "xai" => self.xai_api_key = key,
            other => {
                let env_key = format!("{}_api_key", other);
                if key.is_empty() {
                    self.extras.remove(&env_key);
                } else {
                    self.extras.insert(env_key, key);
                }
            }
        }
    }

    /// Get one provider's key (empty string if unset).
    pub fn get(&self, provider: &str) -> &str {
        match provider {
            "anthropic" => &self.anthropic_api_key,
            "openai" => &self.openai_api_key,
            "xai" => &self.xai_api_key,
            other => self.extras
                .get(&format!("{}_api_key", other))
                .map(String::as_str)
                .unwrap_or(""),
        }
    }

    /// Convert non-empty keys into the env-var pairs cpu expects. Naming
    /// matches what each provider SDK reads at runtime.
    pub fn as_env_pairs(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let push = |out: &mut Vec<(String, String)>, name: &str, val: &str| {
            if !val.is_empty() {
                out.push((name.to_string(), val.to_string()));
            }
        };
        push(&mut out, "ANTHROPIC_API_KEY", &self.anthropic_api_key);
        push(&mut out, "OPENAI_API_KEY", &self.openai_api_key);
        push(&mut out, "XAI_API_KEY", &self.xai_api_key);
        for (k, v) in &self.extras {
            if v.is_empty() { continue; }
            // Convert `foo_api_key` → `FOO_API_KEY` for the env-var convention.
            let env_name = k.to_ascii_uppercase();
            out.push((env_name, v.clone()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let s = Secrets::load(&tmp.path().join("nope.toml")).unwrap();
        assert!(s.anthropic_api_key.is_empty());
        assert!(s.extras.is_empty());
    }

    #[test]
    fn round_trip_anthropic_key() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("secrets.toml");
        let mut s = Secrets::default();
        s.set("anthropic", "sk-test".into());
        s.save(&path).unwrap();
        let loaded = Secrets::load(&path).unwrap();
        assert_eq!(loaded.anthropic_api_key, "sk-test");
        let env = loaded.as_env_pairs();
        assert!(env.iter().any(|(k, v)| k == "ANTHROPIC_API_KEY" && v == "sk-test"));
    }

    #[test]
    fn empty_keys_excluded_from_env_pairs() {
        let s = Secrets::default();
        assert!(s.as_env_pairs().is_empty(), "all-empty secrets must yield no env pairs");
    }

    #[test]
    fn unknown_provider_persists_via_extras() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("secrets.toml");
        let mut s = Secrets::default();
        s.set("gemini", "g-test".into());
        s.save(&path).unwrap();
        let loaded = Secrets::load(&path).unwrap();
        assert_eq!(loaded.get("gemini"), "g-test");
        let env = loaded.as_env_pairs();
        assert!(env.iter().any(|(k, v)| k == "GEMINI_API_KEY" && v == "g-test"));
    }

    #[test]
    fn set_empty_clears_extra() {
        let mut s = Secrets::default();
        s.set("gemini", "g".into());
        assert_eq!(s.get("gemini"), "g");
        s.set("gemini", String::new());
        assert_eq!(s.get("gemini"), "");
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_chmod_600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("secrets.toml");
        let s = Secrets::default();
        s.save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secrets file must be chmod 600 (got {mode:o})");
    }
}

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArnoldConfig {
    /// Path to the 439 cpu binary. Defaults to ~/.arnold/bin/cpu
    #[serde(default = "default_cpu_binary")]
    pub cpu_binary: PathBuf,
    /// Path to the bios binary. Defaults to ~/.arnold/bin/bios
    #[serde(default = "default_bios_binary")]
    pub bios_binary: PathBuf,
    /// Container image reference used by bios for the runtime environment.
    #[serde(default = "default_runtime_image")]
    pub runtime_image: String,
    /// LLM provider (anthropic, openai, etc.)
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    /// Model identifier
    #[serde(default = "default_model")]
    pub model: String,
    /// Additional directories Arnold's jail will allow beyond ~/.arnold/workspace
    #[serde(default)]
    pub allowed_dirs: Vec<PathBuf>,
}

fn default_cpu_binary() -> PathBuf {
    dirs::home_dir().expect("no home dir").join(".arnold/bin/cpu")
}
fn default_bios_binary() -> PathBuf {
    dirs::home_dir().expect("no home dir").join(".arnold/bin/bios")
}
fn default_runtime_image() -> String { "runtime/os:local".to_string() }
fn default_llm_provider() -> String { "anthropic".to_string() }
fn default_model() -> String { "claude-sonnet-4-6".to_string() }

impl Default for ArnoldConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("no home dir");
        Self {
            cpu_binary: home.join(".arnold/bin/cpu"),
            bios_binary: home.join(".arnold/bin/bios"),
            runtime_image: "runtime/os:local".to_string(),
            llm_provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            allowed_dirs: vec![],
        }
    }
}

impl ArnoldConfig {
    pub fn load_or_default() -> anyhow::Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        let path = home.join(".arnold/config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn arnold_dir() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        let dir = home.join(".arnold");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn socket_path() -> anyhow::Result<PathBuf> {
        Ok(Self::arnold_dir()?.join("arnold.sock"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_v0a_config_with_defaults_for_new_fields() {
        // A v0a-shaped config (no bios_binary / runtime_image) must deserialize
        // by falling back to the daemon defaults, instead of crashing arnoldd on
        // startup — the bug that bit the first v0b upgrade.
        let v0a_toml = r#"
            cpu_binary = "/tmp/cpu"
            llm_provider = "anthropic"
            model = "claude-sonnet-4-6"
            allowed_dirs = []
        "#;
        let cfg: ArnoldConfig = toml::from_str(v0a_toml).expect("v0a config must still load");
        assert_eq!(cfg.cpu_binary, PathBuf::from("/tmp/cpu"));
        assert_eq!(cfg.runtime_image, "runtime/os:local");
        assert!(cfg.bios_binary.ends_with(".arnold/bin/bios"));
    }
}

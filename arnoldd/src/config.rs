use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArnoldConfig {
    /// Path to the 439 cpu binary. Defaults to ~/.arnold/bin/cpu
    pub cpu_binary: PathBuf,
    /// Path to the bios binary. Defaults to ~/.arnold/bin/bios
    pub bios_binary: PathBuf,
    /// Container image reference used by bios for the runtime environment.
    pub runtime_image: String,
    /// LLM provider (anthropic, openai, etc.)
    pub llm_provider: String,
    /// Model identifier
    pub model: String,
    /// Additional directories Arnold's jail will allow beyond ~/.arnold/workspace
    pub allowed_dirs: Vec<PathBuf>,
}

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

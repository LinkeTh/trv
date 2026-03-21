use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: Option<PathBuf>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trv")
        .join("config.toml")
}

pub fn load() -> Result<AppConfig> {
    let path = config_path();
    if !path.is_file() {
        return Ok(AppConfig::default());
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read config {:?}", path))?;
    let cfg: AppConfig =
        toml::from_str(&raw).with_context(|| format!("could not parse config {:?}", path))?;
    Ok(cfg)
}

pub fn save(cfg: &AppConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create config directory {:?}", parent))?;
    }

    let toml_str = toml::to_string_pretty(cfg).context("could not serialize config")?;
    let tmp_name = format!(
        "{}.tmp.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml"),
        std::process::id()
    );
    let tmp_path = path.with_file_name(tmp_name);

    std::fs::write(&tmp_path, toml_str)
        .with_context(|| format!("could not write temporary config {:?}", tmp_path))?;

    std::fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "could not atomically replace config {:?} with {:?}",
            path, tmp_path
        )
    })?;

    Ok(())
}

pub fn get_default_theme_path() -> Result<Option<PathBuf>> {
    Ok(load()?.theme)
}

pub fn set_default_theme_path(path: &Path) -> Result<()> {
    let mut cfg = load()?;
    cfg.theme = Some(path.to_path_buf());
    save(&cfg)
}

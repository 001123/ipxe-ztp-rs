//! Application data shared through `AppContext::shared_store`.
//!
//! The data dir is resolved once at boot by
//! [`crate::initializers::data_dir::DataDirInitializer`] and published here,
//! so the download worker and the iPXE file-serving controller read the same
//! location without recomputing it.

use std::path::{Path, PathBuf};

use loco_rs::{config::Config, prelude::*};

/// The directory where OS artifacts (`<data_dir>/os/<id>/...`) are stored.
///
/// Resolved from a `settings.data_dir` config override, or the platform
/// standard data directory via `directories::ProjectDirs`.
#[derive(Debug, Clone)]
pub struct DataDir(pub PathBuf);

impl DataDir {
    /// Resolve the data dir from config: the `settings.data_dir` override when
    /// present, otherwise the platform data directory for this app.
    ///
    /// # Errors
    ///
    /// When `settings:` is present but not an object with a string
    /// `data_dir` value.
    pub fn resolve_from_config(config: &Config) -> Result<Self> {
        if let Some(settings) = &config.settings {
            let raw = settings
                .get("data_dir")
                .ok_or_else(|| Error::string("`settings.data_dir` is missing"))?;
            let path = raw
                .as_str()
                .ok_or_else(|| Error::string("`settings.data_dir` must be a string"))?;
            Ok(Self(PathBuf::from(path)))
        } else {
            Ok(Self(Self::default_dir()))
        }
    }

    /// Platform-standard fallback: `directories::ProjectDirs` data dir.
    #[must_use]
    pub fn default_dir() -> PathBuf {
        directories::ProjectDirs::from("rs", "", env!("CARGO_CRATE_NAME"))
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("data"))
    }

    /// Parent directory of all per-OS artifact folders.
    #[must_use]
    pub fn os_dir(&self) -> PathBuf {
        self.0.join("os")
    }

    /// Artifact folder for one OS version: `<data_dir>/os/<id>/`.
    #[must_use]
    pub fn os_version_dir(&self, os_version_id: i64) -> PathBuf {
        self.os_dir().join(os_version_id.to_string())
    }

    /// Read the data dir published by the initializer.
    ///
    /// # Errors
    ///
    /// When the initializer did not run (it always does at boot).
    pub fn from_context(ctx: &AppContext) -> Result<Self> {
        ctx.shared_store
            .get::<DataDir>()
            .ok_or_else(|| Error::string("data dir not initialized (missing DataDirInitializer)"))
    }

    /// Ensure the per-OS artifact directory exists.
    ///
    /// # Errors
    ///
    /// When the directory cannot be created.
    pub async fn ensure_os_version_dir(&self, os_version_id: i64) -> Result<PathBuf> {
        let dir = self.os_version_dir(os_version_id);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }
}

impl std::ops::Deref for DataDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

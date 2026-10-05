use std::str::FromStr;

use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

pub use super::_entities::os_versions::{self, ActiveModel, Entity, Model};
pub type OsVersions = Entity;

/// Progress of a mirror download, persisted as a plain string column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Ready,
    Failed,
}

impl DownloadStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DownloadStatus {
    type Err = ModelError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "downloading" => Ok(Self::Downloading),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            _ => Err(ModelError::msg(
                format!("invalid download status: {s}").as_str(),
            )),
        }
    }
}

impl From<DownloadStatus> for sea_orm::ActiveValue<String> {
    fn from(status: DownloadStatus) -> Self {
        ActiveValue::set(status.as_str().to_string())
    }
}

/// How the installer fetches its live medium: `online` casper downloads the
/// ISO from the mirror into the client's RAM, `offline` fetches the locally
/// downloaded copy, `nfs` mounts the extracted casper directory over NFS so
/// low-RAM clients (under ~4 GiB) never copy the medium into RAM at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootMode {
    #[default]
    Online,
    Offline,
    Nfs,
}

impl BootMode {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Nfs => "nfs",
        }
    }
}

impl std::fmt::Display for BootMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BootMode {
    type Err = ModelError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "online" => Ok(Self::Online),
            "offline" => Ok(Self::Offline),
            "nfs" => Ok(Self::Nfs),
            _ => Err(ModelError::msg(format!("invalid boot mode: {s}").as_str())),
        }
    }
}

impl From<BootMode> for sea_orm::ActiveValue<String> {
    fn from(mode: BootMode) -> Self {
        ActiveValue::set(mode.as_str().to_string())
    }
}

/// Canonical local file name of the netboot kernel inside
/// `<data_dir>/os/<id>/`.
pub const KERNEL_FILE: &str = "vmlinuz";
/// Canonical local file name of the netboot initrd.
pub const INITRD_FILE: &str = "initrd";

/// Params for creating an OS version.
#[derive(Debug, Validate, Deserialize)]
pub struct CreateParams {
    #[validate(length(min = 1, message = "Name is required."))]
    pub name: String,
    #[validate(length(min = 1, message = "Version is required."))]
    pub version: String,
    #[validate(length(min = 1, message = "Arch is required."))]
    pub arch: String,
    #[validate(url(message = "kernel_url must be a URL."))]
    pub kernel_url: String,
    #[validate(url(message = "initrd_url must be a URL."))]
    pub initrd_url: String,
    #[validate(url(message = "iso_url must be a URL."))]
    pub iso_url: String,
    #[validate(custom(function = "validate_boot_mode"))]
    pub boot_mode: String,
}

/// Accepts only known boot mode strings (see [`BootMode`]).
fn validate_boot_mode(mode: &str) -> Result<(), validator::ValidationError> {
    BootMode::from_str(mode)
        .map(|_| ())
        .map_err(|_| validator::ValidationError::new("boot_mode must be online, offline or nfs"))
}

/// Params for updating an OS version (all fields optional).
#[derive(Debug, Validate, Deserialize)]
pub struct UpdateParams {
    #[validate(length(min = 1, message = "Name cannot be empty."))]
    pub name: Option<String>,
    #[validate(length(min = 1, message = "Version cannot be empty."))]
    pub version: Option<String>,
    #[validate(length(min = 1, message = "Arch cannot be empty."))]
    pub arch: Option<String>,
    #[validate(url(message = "kernel_url must be a URL."))]
    pub kernel_url: Option<String>,
    #[validate(url(message = "initrd_url must be a URL."))]
    pub initrd_url: Option<String>,
    #[validate(url(message = "iso_url must be a URL."))]
    pub iso_url: Option<String>,
    #[validate(custom(function = "validate_boot_mode_opt"))]
    pub boot_mode: Option<String>,
}

/// Accepts only known boot mode strings when the field is present.
///
/// With `validator` 0.20, `#[validate(custom(...))]` on an `Option<String>`
/// field only fires for `Some(..)` and passes `&&String`.
fn validate_boot_mode_opt(mode: &&String) -> Result<(), validator::ValidationError> {
    validate_boot_mode(mode)
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            // new rows always start a fresh download lifecycle
            self.download_status = DownloadStatus::Pending.into();
            self.bytes_downloaded = ActiveValue::set(0);
            self.total_bytes = ActiveValue::set(0);
            if self.boot_mode.is_not_set() {
                self.boot_mode = BootMode::Online.into();
            }
        }
        if !insert && self.updated_at.is_unchanged() {
            self.updated_at = ActiveValue::set(chrono::Utc::now().into());
        }
        Ok(self)
    }
}

impl Model {
    /// The last path segment of a URL — used to derive the ISO's local file
    /// name from `iso_url`.
    #[must_use]
    pub fn file_name_from_url(url: &str) -> Option<String> {
        let name = url.rsplit('/').next().unwrap_or(url);
        let name = name.split(['?', '#']).next().unwrap_or(name);
        if name.is_empty() || name.contains(['/', '\\']) {
            None
        } else {
            Some(name.to_string())
        }
    }

    /// The local file name this OS version's ISO is stored (and served)
    /// under, derived from `iso_url`.
    #[must_use]
    pub fn iso_file_name(&self) -> String {
        Self::file_name_from_url(&self.iso_url).unwrap_or_else(|| "iso".to_string())
    }

    /// Whether every artifact is downloaded locally. Required to boot an
    /// installer in any boot mode: kernel and initrd are always served from
    /// this server's local mirror (they are extracted from the ISO — no mirror
    /// hosts them).
    #[must_use]
    pub fn artifacts_ready(&self) -> bool {
        DownloadStatus::from_str(&self.download_status)
            .is_ok_and(|status| status == DownloadStatus::Ready)
    }

    /// Sanitized path of a served artifact, rejecting path traversal: the
    /// file name must be a bare name inside the OS version's folder.
    #[must_use]
    pub fn artifact_path(
        &self,
        data_dir: &std::path::Path,
        file: &str,
    ) -> Option<std::path::PathBuf> {
        if file.contains('/') || file.contains('\\') || file.starts_with('.') || file.is_empty() {
            return None;
        }
        Some(data_dir.join("os").join(self.id.to_string()).join(file))
    }

    /// Directory holding the extracted NFS medium (`casper/`) for this OS
    /// version — NFS boot mounts it directly, so low-RAM clients never copy
    /// the medium into RAM. The dir must contain at least one `*.squashfs`.
    #[must_use]
    pub fn nfs_medium_dir(&self, data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("os").join(self.id.to_string()).join("casper")
    }
}

impl ActiveModel {
    /// Creates a new OS version ready to be downloaded.
    ///
    /// # Errors
    ///
    /// When validation fails or the DB insert fails.
    pub async fn create(db: &DatabaseConnection, params: &CreateParams) -> ModelResult<Model> {
        ValidatorTrait::validate(&params).map_err(ModelError::from)?;
        let os_version = ActiveModel {
            name: ActiveValue::set(params.name.clone()),
            version: ActiveValue::set(params.version.clone()),
            arch: ActiveValue::set(params.arch.clone()),
            kernel_url: ActiveValue::set(params.kernel_url.clone()),
            initrd_url: ActiveValue::set(params.initrd_url.clone()),
            iso_url: ActiveValue::set(params.iso_url.clone()),
            boot_mode: ActiveValue::set(params.boot_mode.clone()),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(os_version)
    }

    /// Applies the present fields of `params` and persists.
    ///
    /// # Errors
    ///
    /// When validation fails or the DB update fails.
    pub async fn update_from_params(
        mut self,
        db: &DatabaseConnection,
        params: &UpdateParams,
    ) -> ModelResult<Model> {
        ValidatorTrait::validate(&params).map_err(ModelError::from)?;
        if let Some(name) = &params.name {
            self.name = ActiveValue::set(name.clone());
        }
        if let Some(version) = &params.version {
            self.version = ActiveValue::set(version.clone());
        }
        if let Some(arch) = &params.arch {
            self.arch = ActiveValue::set(arch.clone());
        }
        if let Some(kernel_url) = &params.kernel_url {
            self.kernel_url = ActiveValue::set(kernel_url.clone());
        }
        if let Some(initrd_url) = &params.initrd_url {
            self.initrd_url = ActiveValue::set(initrd_url.clone());
        }
        if let Some(iso_url) = &params.iso_url {
            self.iso_url = ActiveValue::set(iso_url.clone());
        }
        if let Some(boot_mode) = &params.boot_mode {
            self.boot_mode = ActiveValue::set(boot_mode.clone());
        }
        Ok(self.update(db).await?)
    }

    /// Marks the download as started, resetting progress counters.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn begin_download(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.download_status = DownloadStatus::Downloading.into();
        self.bytes_downloaded = ActiveValue::set(0);
        self.total_bytes = ActiveValue::set(0);
        self.error = ActiveValue::Set(None);
        Ok(self.update(db).await?)
    }

    /// Records cumulative progress across all artifacts of this OS version.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn set_download_progress(
        mut self,
        db: &DatabaseConnection,
        bytes_downloaded: i64,
        total_bytes: i64,
    ) -> ModelResult<Model> {
        self.download_status = DownloadStatus::Downloading.into();
        self.bytes_downloaded = ActiveValue::set(bytes_downloaded);
        self.total_bytes = ActiveValue::set(total_bytes);
        Ok(self.update(db).await?)
    }

    /// Marks the download complete.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn mark_ready(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.download_status = DownloadStatus::Ready.into();
        self.error = ActiveValue::Set(None);
        Ok(self.update(db).await?)
    }

    /// Marks the download failed with a human-readable error.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn mark_failed(mut self, db: &DatabaseConnection, error: &str) -> ModelResult<Model> {
        self.download_status = DownloadStatus::Failed.into();
        self.error = ActiveValue::set(Some(error.to_string()));
        Ok(self.update(db).await?)
    }
}

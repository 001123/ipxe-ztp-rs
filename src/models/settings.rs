use loco_rs::prelude::*;
use serde::Deserialize;

pub use super::_entities::settings::{self, ActiveModel, Entity, Model};
pub type Settings = Entity;

/// Known setting keys. Anything outside this set is rejected by the admin API
/// (typos in keys would otherwise silently change nothing).
pub mod keys {
    /// ID of the OS version offered to freshly approved machines.
    pub const DEFAULT_OS_VERSION_ID: &str = "default_os_version_id";
    /// Username created by the autoinstall cloud-init.
    pub const INSTALL_USERNAME: &str = "install_username";
    /// Password hash (mkpasswd-style) handed to the autoinstall identity.
    pub const INSTALL_PASSWORD_HASH: &str = "install_password_hash";
    /// Write-only plaintext install password — the admin API accepts it,
    /// hashes it into [`INSTALL_PASSWORD_HASH`](INSTALL_PASSWORD_HASH) and
    /// never returns it.
    pub const INSTALL_PASSWORD: &str = "install_password";
    /// SSH public key installed for the autoinstall user.
    pub const SSH_KEY: &str = "ssh_key";
    /// Timezone configured in the installed system.
    pub const TIMEZONE: &str = "timezone";
    /// Base URL (`http://host:port`) iPXE clients use to reach this server.
    pub const PUBLIC_HOST: &str = "public_host";
    /// Hostname/IP clients use to reach the NFS export (`netboot=nfs`).
    pub const NFS_HOST: &str = "nfs_host";
    /// Absolute path exported over NFS that mirrors the data dir.
    pub const NFS_EXPORT_ROOT: &str = "nfs_export_root";
    /// Default package list (one per line) installed after the OS install.
    pub const PACKAGES: &str = "packages";
    /// Default install disk pinned in the autoinstall storage config.
    pub const INSTALL_DISK: &str = "install_disk";

    #[must_use]
    pub fn all() -> &'static [&'static str] {
        &[
            DEFAULT_OS_VERSION_ID,
            INSTALL_USERNAME,
            INSTALL_PASSWORD_HASH,
            INSTALL_PASSWORD,
            SSH_KEY,
            TIMEZONE,
            PUBLIC_HOST,
            NFS_HOST,
            NFS_EXPORT_ROOT,
            PACKAGES,
            INSTALL_DISK,
        ]
    }

    #[must_use]
    pub fn is_known(key: &str) -> bool {
        all().contains(&key)
    }
}

/// Default username created on installed machines.
pub const DEFAULT_INSTALL_USERNAME: &str = "ubuntu";
/// Default timezone of installed machines.
pub const DEFAULT_TIMEZONE: &str = "UTC";

#[derive(Debug, Validate, Deserialize)]
pub struct Validator {
    #[validate(length(min = 1, message = "Key is required."))]
    pub key: String,
}

impl Validatable for ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(Validator {
            key: self.key.as_ref().to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, _insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        self.validate()?;
        if self.updated_at.is_unchanged() {
            self.updated_at = ActiveValue::set(chrono::Utc::now().into());
        }
        Ok(self)
    }
}

impl Model {
    /// Finds a setting row by key.
    ///
    /// # Errors
    ///
    /// Returns `ModelError::EntityNotFound` when the key has no row.
    pub async fn find_by_key(db: &DatabaseConnection, key: &str) -> ModelResult<Self> {
        let setting = settings::Entity::find()
            .filter(
                model::query::condition()
                    .eq(settings::Column::Key, key)
                    .build(),
            )
            .one(db)
            .await?;
        setting.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// Reads the raw string value of a key, `None` when unset.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get(db: &DatabaseConnection, key: &str) -> ModelResult<Option<String>> {
        Ok(Self::find_by_key(db, key).await.map(|s| s.value).ok())
    }

    /// Reads a key and parses it; `None` when unset or unparseable.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_parsed<T: std::str::FromStr>(
        db: &DatabaseConnection,
        key: &str,
    ) -> ModelResult<Option<T>> {
        let raw = Self::get(db, key).await?;
        Ok(raw.and_then(|v| v.parse::<T>().ok()))
    }

    /// Upserts a key with a raw string value.
    ///
    /// # Errors
    ///
    /// When the DB write fails.
    pub async fn set(db: &DatabaseConnection, key: &str, value: &str) -> ModelResult<Self> {
        match Self::find_by_key(db, key).await {
            Ok(existing) => {
                let mut am: ActiveModel = existing.into();
                am.value = ActiveValue::set(value.to_string());
                am.update(db).await.map_err(Into::into)
            }
            Err(ModelError::EntityNotFound) => {
                let am = ActiveModel {
                    key: ActiveValue::set(key.to_string()),
                    value: ActiveValue::set(value.to_string()),
                    ..Default::default()
                };
                am.insert(db).await.map_err(Into::into)
            }
            Err(e) => Err(e),
        }
    }

    /// The OS version id offered to newly approved machines.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_default_os_version_id(db: &DatabaseConnection) -> ModelResult<Option<i64>> {
        Self::get_parsed(db, keys::DEFAULT_OS_VERSION_ID).await
    }

    /// Username created by autoinstall; falls back to
    /// [`DEFAULT_INSTALL_USERNAME`].
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_install_username(db: &DatabaseConnection) -> ModelResult<String> {
        Ok(Self::get(db, keys::INSTALL_USERNAME)
            .await?
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_INSTALL_USERNAME.to_string()))
    }

    /// Password hash handed to the autoinstall identity (empty when unset —
    /// the installed user then has a locked password).
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_install_password_hash(db: &DatabaseConnection) -> ModelResult<String> {
        Ok(Self::get(db, keys::INSTALL_PASSWORD_HASH)
            .await?
            .unwrap_or_default())
    }

    /// Hashes a plaintext install password into SHA-512 crypt (`$6$…` — the
    /// format subiquity's autoinstall identity accepts) and stores it under
    /// [`keys::INSTALL_PASSWORD_HASH`].
    ///
    /// # Errors
    ///
    /// When hashing or the DB write fails.
    pub async fn set_install_password(
        db: &DatabaseConnection,
        password: &str,
    ) -> ModelResult<Self> {
        use sha_crypt::PasswordHasher;

        let hash = sha_crypt::ShaCrypt::SHA512
            .hash_password(password.as_bytes())
            .map_err(|e| ModelError::Message(format!("hashing install password failed: {e}")))?
            .to_string();
        Self::set(db, keys::INSTALL_PASSWORD_HASH, &hash).await
    }

    /// SSH public key installed for the autoinstall user (empty when unset).
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_ssh_key(db: &DatabaseConnection) -> ModelResult<String> {
        Ok(Self::get(db, keys::SSH_KEY).await?.unwrap_or_default())
    }

    /// Timezone of installed machines; falls back to [`DEFAULT_TIMEZONE`].
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_timezone(db: &DatabaseConnection) -> ModelResult<String> {
        Ok(Self::get(db, keys::TIMEZONE)
            .await?
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string()))
    }

    /// Public host override; `None` when unset — callers fall back to config
    /// (`server.host:port`).
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_public_host(db: &DatabaseConnection) -> ModelResult<Option<String>> {
        Ok(Self::get(db, keys::PUBLIC_HOST)
            .await?
            .filter(|v| !v.is_empty()))
    }

    /// Hostname/IP clients use for the NFS medium; `None` when unset —
    /// callers fall back to the host part of the public host.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_nfs_host(db: &DatabaseConnection) -> ModelResult<Option<String>> {
        Ok(Self::get(db, keys::NFS_HOST)
            .await?
            .filter(|v| !v.is_empty()))
    }

    /// Absolute path the operator exports over NFS in place of the data dir;
    /// `None` when unset — callers fall back to the data dir itself.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_nfs_export_root(db: &DatabaseConnection) -> ModelResult<Option<String>> {
        Ok(Self::get(db, keys::NFS_EXPORT_ROOT)
            .await?
            .filter(|v| !v.is_empty()))
    }

    /// Default package list installed after the OS install, one package per
    /// line (empty when unset).
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_packages(db: &DatabaseConnection) -> ModelResult<String> {
        Ok(Self::get(db, keys::PACKAGES).await?.unwrap_or_default())
    }

    /// Default install disk pinned in the autoinstall storage config; `None`
    /// when unset — callers then let autoinstall pick the largest disk.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn get_install_disk(db: &DatabaseConnection) -> ModelResult<Option<String>> {
        Ok(Self::get(db, keys::INSTALL_DISK)
            .await?
            .filter(|v| !v.is_empty()))
    }
}

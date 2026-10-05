use std::str::FromStr;
use std::sync::LazyLock;

use chrono::Utc;
use loco_rs::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub use super::_entities::machines::{self, ActiveModel, Column, Entity, Model};
pub type Machines = Entity;

/// RFC 1123 hostname: dot-separated labels of letters, digits and hyphens,
/// each label 1–63 characters and not starting or ending with a hyphen.
/// (Total length is capped separately by the validator's length rule.)
static HOSTNAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?",
        r"(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*\z"
    ))
    .expect("hostname regex must be valid")
});

/// Lifecycle of a machine, persisted as a plain string column.
///
/// `pending` (new MAC seen) → `approved` (admin picked an OS) → `installing`
/// (iPXE served the install script; `install_started_at` recorded) →
/// `installed` (autoinstall late-command called back; `installed_at`
/// recorded). `failed` is a terminal state an admin can only reset out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineStatus {
    Pending,
    Approved,
    Installing,
    Installed,
    Failed,
}

impl MachineStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Pending,
            Self::Approved,
            Self::Installing,
            Self::Installed,
            Self::Failed,
        ]
    }
}

impl std::fmt::Display for MachineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MachineStatus {
    type Err = ModelError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "installing" => Ok(Self::Installing),
            "installed" => Ok(Self::Installed),
            "failed" => Ok(Self::Failed),
            _ => Err(ModelError::msg(
                format!("invalid machine status: {s}").as_str(),
            )),
        }
    }
}

impl From<MachineStatus> for sea_orm::ActiveValue<String> {
    fn from(status: MachineStatus) -> Self {
        ActiveValue::set(status.as_str().to_string())
    }
}

/// Normalize a MAC address to the canonical `AA:BB:CC:DD:EE:FF` form so a
/// machine registered from dnsmasq (`AA-BB-CC-DD-EE-FF`), iPXE
/// (`aa:bb:cc:dd:ee:ff`), or the admin UI are the same row — and all MACs
/// display uniformly uppercase.
#[must_use]
pub fn normalize_mac(mac: &str) -> String {
    mac.trim().to_uppercase().replace(['-', '.'], ":")
}

#[derive(Debug, Validate, Deserialize)]
pub struct Validator {
    #[validate(length(
        min = 12,
        max = 32,
        message = "MAC address must look like a MAC address."
    ))]
    pub mac: String,
    #[validate(length(max = 253, message = "Name is too long to be a hostname."))]
    #[validate(regex(path = *HOSTNAME_RE, message = "Name must be a valid hostname."))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 64, message = "Username must be 1-64 characters."))]
    pub username: Option<String>,
    #[validate(length(max = 4096, message = "SSH key is too long."))]
    pub ssh_key: Option<String>,
    #[validate(length(max = 64, message = "Disk path is too long."))]
    pub install_disk: Option<String>,
    #[validate(length(max = 2048, message = "Cloud-init URL is too long."))]
    pub cloudinit_url: Option<String>,
    #[validate(length(max = 64, message = "Timezone is too long."))]
    pub timezone: Option<String>,
    #[validate(custom(function = "validate_packages"))]
    pub packages: Option<String>,
}

/// Validator glue for the optional stored package list: validate every entry
/// with [`crate::models::packages::validate_list`], mapping the error into
/// the `validator` crate's custom-rule shape. The double reference matches
/// the crate's generated call for `Option<String>` fields (the inner value,
/// re-borrowed); `None` is skipped by the derive itself.
fn validate_packages(value: &&String) -> std::result::Result<(), validator::ValidationError> {
    crate::models::packages::validate_list(value)
        .map_err(|e| validator::ValidationError::new("packages").with_message(e.into()))
}

/// Normalizes an optional plaintext password param: trimmed, blank → `None`
/// (a blank password means "keep the current password source").
fn params_password(password: &Option<String>) -> Option<&str> {
    password.as_deref().map(str::trim).filter(|p| !p.is_empty())
}

/// Hashes a plaintext install password into SHA-512 crypt (`$6$…` — the
/// format subiquity's autoinstall identity accepts). Same scheme as the
/// global settings password.
///
/// # Errors
///
/// When the hashing itself fails.
fn hash_install_password(password: &str) -> ModelResult<String> {
    use sha_crypt::PasswordHasher;

    sha_crypt::ShaCrypt::SHA512
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| ModelError::Message(format!("hashing install password failed: {e}")))
}

impl Validatable for ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(Validator {
            mac: self.mac.as_ref().to_owned(),
            // optional columns may be NotSet (fresh rows) — try_as_ref does
            // not panic there, and validator skips None fields
            name: self.name.try_as_ref().cloned().flatten(),
            username: self.username.try_as_ref().cloned().flatten(),
            ssh_key: self.ssh_key.try_as_ref().cloned().flatten(),
            install_disk: self.install_disk.try_as_ref().cloned().flatten(),
            cloudinit_url: self.cloudinit_url.try_as_ref().cloned().flatten(),
            timezone: self.timezone.try_as_ref().cloned().flatten(),
            packages: self.packages.try_as_ref().cloned().flatten(),
        })
    }
}

/// Params for manually registering a machine.
#[derive(Debug, Validate, Deserialize)]
pub struct CreateParams {
    #[validate(length(
        min = 12,
        max = 32,
        message = "MAC address must look like a MAC address."
    ))]
    pub mac: String,
    #[validate(length(max = 253, message = "Name is too long to be a hostname."))]
    #[validate(regex(path = *HOSTNAME_RE, message = "Name must be a valid hostname."))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 64, message = "Username must be 1-64 characters."))]
    pub username: Option<String>,
    #[validate(length(max = 4096, message = "SSH key is too long."))]
    pub ssh_key: Option<String>,
    #[validate(length(max = 2000, message = "Notes are too long."))]
    pub notes: Option<String>,
    #[validate(length(max = 64, message = "Disk path is too long."))]
    pub install_disk: Option<String>,
    #[validate(length(max = 2048, message = "Cloud-init URL is too long."))]
    pub cloudinit_url: Option<String>,
    #[validate(length(max = 64, message = "Timezone is too long."))]
    pub timezone: Option<String>,
    #[validate(custom(function = "validate_packages"))]
    pub packages: Option<String>,
    #[validate(length(max = 256, message = "Password is too long."))]
    pub password: Option<String>,
}

/// Params for editing a machine's install identity (all optional; `mac` and
/// the lifecycle fields are deliberately not editable here).
#[derive(Debug, Validate, Deserialize)]
pub struct UpdateParams {
    #[validate(length(max = 253, message = "Name is too long to be a hostname."))]
    #[validate(regex(path = *HOSTNAME_RE, message = "Name must be a valid hostname."))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 64, message = "Username must be 1-64 characters."))]
    pub username: Option<String>,
    #[validate(length(max = 4096, message = "SSH key is too long."))]
    pub ssh_key: Option<String>,
    #[validate(length(max = 2000, message = "Notes are too long."))]
    pub notes: Option<String>,
    #[validate(length(max = 64, message = "Disk path is too long."))]
    pub install_disk: Option<String>,
    #[validate(length(max = 2048, message = "Cloud-init URL is too long."))]
    pub cloudinit_url: Option<String>,
    #[validate(length(max = 64, message = "Timezone is too long."))]
    pub timezone: Option<String>,
    #[validate(custom(function = "validate_packages"))]
    pub packages: Option<String>,
    /// Write-only plaintext password — hashed into the stored
    /// `password_hash` and never persisted as-is. `None`/empty keeps the
    /// current state.
    #[validate(length(max = 256, message = "Password is too long."))]
    pub password: Option<String>,
    /// `true` clears the machine's own password so it falls back to the
    /// global settings password again. Ignored when `password` is present.
    pub password_reset: Option<bool>,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        self.mac = ActiveValue::set(normalize_mac(&self.mac.as_ref().clone()));
        self.validate()?;
        if !insert && self.updated_at.is_unchanged() {
            self.updated_at = ActiveValue::set(Utc::now().into());
        }
        Ok(self)
    }
}

impl Model {
    /// Finds a machine by MAC address, normalizing it first.
    ///
    /// # Errors
    ///
    /// Returns `ModelError::EntityNotFound` when no machine has this MAC.
    pub async fn find_by_mac(db: &DatabaseConnection, mac: &str) -> ModelResult<Self> {
        let machine = machines::Entity::find()
            .filter(
                model::query::condition()
                    .eq(machines::Column::Mac, normalize_mac(mac))
                    .build(),
            )
            .one(db)
            .await?;
        machine.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// Finds a machine by MAC or registers it as `pending` on first sight.
    /// This is the entry point of the whole ZTP flow — iPXE hits it on every
    /// boot.
    ///
    /// # Errors
    ///
    /// When the DB query fails or the insert violates validation.
    pub async fn find_or_register_by_mac(db: &DatabaseConnection, mac: &str) -> ModelResult<Self> {
        match Self::find_by_mac(db, mac).await {
            Ok(machine) => Ok(machine),
            Err(ModelError::EntityNotFound) => {
                let machine = machines::ActiveModel {
                    mac: ActiveValue::set(normalize_mac(mac)),
                    status: MachineStatus::Pending.into(),
                    ..Default::default()
                }
                .insert(db)
                .await?;
                tracing::info!(mac = %machine.mac, "registered new machine as pending");
                Ok(machine)
            }
            Err(e) => Err(e),
        }
    }

    /// Lists machines, optionally filtered by status, newest activity first.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn list(
        db: &DatabaseConnection,
        status: Option<MachineStatus>,
    ) -> ModelResult<Vec<Self>> {
        let mut select = machines::Entity::find();
        if let Some(status) = status {
            select = select.filter(
                model::query::condition()
                    .eq(machines::Column::Status, status.as_str())
                    .build(),
            );
        }
        Ok(select
            .order_by_desc(machines::Column::LastSeenAt)
            .order_by_desc(machines::Column::Id)
            .all(db)
            .await?)
    }
}

impl ActiveModel {
    /// Manually registers a machine by MAC as `pending`, with optional install
    /// identity. Rejects a MAC that is already registered.
    ///
    /// # Errors
    ///
    /// When validation fails, the MAC already exists, or the DB fails.
    pub async fn create(db: &DatabaseConnection, params: &CreateParams) -> ModelResult<Model> {
        ValidatorTrait::validate(&params).map_err(ModelError::from)?;
        let mac = normalize_mac(&params.mac);
        // check-then-insert inside a transaction: a duplicate MAC is a race,
        // not a validation
        let txn = db.begin().await?;
        if machines::Entity::find()
            .filter(
                model::query::condition()
                    .eq(Column::Mac, mac.clone())
                    .build(),
            )
            .one(&txn)
            .await?
            .is_some()
        {
            return Err(ModelError::EntityAlreadyExists {});
        }
        let machine = ActiveModel {
            mac: ActiveValue::set(mac),
            name: ActiveValue::set(params.name.clone()),
            username: ActiveValue::set(params.username.clone()),
            ssh_key: ActiveValue::set(params.ssh_key.clone()),
            notes: ActiveValue::set(params.notes.clone()),
            install_disk: ActiveValue::set(params.install_disk.clone()),
            cloudinit_url: ActiveValue::set(params.cloudinit_url.clone()),
            timezone: ActiveValue::set(params.timezone.clone()),
            // a write-only plaintext is hashed into the stored hash; blank or
            // absent leaves the machine on the global settings password
            password_hash: ActiveValue::set(match params_password(&params.password) {
                Some(p) => Some(hash_install_password(p)?),
                None => None,
            }),
            packages: ActiveValue::set(params.packages.clone()),
            status: MachineStatus::Pending.into(),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        txn.commit().await?;
        tracing::info!(machine_id = machine.id, mac = %machine.mac, "machine manually registered");
        Ok(machine)
    }

    /// Edits the machine's install identity. Only present fields change; the
    /// lifecycle (`status`, approved OS, timestamps) is untouched.
    ///
    /// # Errors
    ///
    /// When validation fails or the DB update fails.
    pub async fn update_info(
        mut self,
        db: &DatabaseConnection,
        params: &UpdateParams,
    ) -> ModelResult<Model> {
        ValidatorTrait::validate(&params).map_err(ModelError::from)?;
        if let Some(name) = &params.name {
            self.name = ActiveValue::set(Some(name.clone()));
        }
        if let Some(username) = &params.username {
            self.username = ActiveValue::set(Some(username.clone()));
        }
        if let Some(ssh_key) = &params.ssh_key {
            self.ssh_key = ActiveValue::set(Some(ssh_key.clone()));
        }
        if let Some(notes) = &params.notes {
            self.notes = ActiveValue::set(Some(notes.clone()));
        }
        if let Some(install_disk) = &params.install_disk {
            self.install_disk = ActiveValue::set(Some(install_disk.clone()));
        }
        if let Some(cloudinit_url) = &params.cloudinit_url {
            self.cloudinit_url = ActiveValue::set(Some(cloudinit_url.clone()));
        }
        if let Some(timezone) = &params.timezone {
            self.timezone = ActiveValue::set(Some(timezone.clone()));
        }
        // write-only password: a present plaintext replaces the machine's own
        // hash; a reset (with no plaintext) clears it back to the global
        // settings fallback. Reset is ignored when both are sent.
        match params_password(&params.password) {
            Some(password) => {
                self.password_hash = ActiveValue::set(Some(hash_install_password(password)?));
            }
            None if params.password_reset == Some(true) => {
                self.password_hash = ActiveValue::Set(None);
            }
            None => {}
        }
        if let Some(packages) = &params.packages {
            self.packages = ActiveValue::set(Some(packages.clone()));
        }
        Ok(self.update(db).await?)
    }

    /// Touches `last_seen_at` — every iPXE boot hit does this.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn touch_last_seen(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.last_seen_at = ActiveValue::set(Some(Utc::now().naive_utc()));
        Ok(self.update(db).await?)
    }

    /// Approves the machine for install with the given OS version.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn approve(
        mut self,
        db: &DatabaseConnection,
        os_version_id: i64,
    ) -> ModelResult<Model> {
        self.status = MachineStatus::Approved.into();
        self.os_version_id = ActiveValue::set(Some(os_version_id));
        self.approved_at = ActiveValue::set(Some(Utc::now().naive_utc()));
        // A fresh approval cycle invalidates the previous run's timing.
        self.install_started_at = ActiveValue::Set(None);
        Ok(self.update(db).await?)
    }

    /// Marks the machine as actively installing (the install script was
    /// handed out) and records the install start time.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn mark_installing(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.status = MachineStatus::Installing.into();
        self.install_started_at = ActiveValue::set(Some(Utc::now().naive_utc()));
        Ok(self.update(db).await?)
    }

    /// Marks the machine as installed (the installer called back).
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn mark_installed(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.status = MachineStatus::Installed.into();
        self.installed_at = ActiveValue::set(Some(Utc::now().naive_utc()));
        Ok(self.update(db).await?)
    }

    /// Marks the machine as failed (e.g. the install never called back).
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn mark_failed(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.status = MachineStatus::Failed.into();
        Ok(self.update(db).await?)
    }

    /// Resets the machine to `pending`, clearing the approved OS so the admin
    /// can pick a new one.
    ///
    /// # Errors
    ///
    /// When the DB update fails.
    pub async fn reset(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.status = MachineStatus::Pending.into();
        self.os_version_id = ActiveValue::Set(None);
        self.approved_at = ActiveValue::Set(None);
        self.installed_at = ActiveValue::Set(None);
        self.install_started_at = ActiveValue::Set(None);
        Ok(self.update(db).await?)
    }
}

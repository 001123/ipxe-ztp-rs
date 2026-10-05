use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::settings;

/// Every known setting, with the model-level defaults already applied.
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct SettingsResponse {
    #[ts(type = "number | null")]
    pub default_os_version_id: Option<i64>,
    pub install_username: String,
    /// Whether an install password has been set (the hash itself is never
    /// exposed). Lets the UI distinguish "unset" from "leave blank to keep".
    #[ts(type = "boolean")]
    pub install_password_set: bool,
    pub ssh_key: String,
    pub timezone: String,
    /// Default package list installed after the OS install, one per line.
    pub packages: String,
    /// Default install disk pinned in the autoinstall storage config.
    #[ts(type = "string | null")]
    pub install_disk: Option<String>,
    #[ts(type = "string | null")]
    pub public_host: Option<String>,
    #[ts(type = "string | null")]
    pub nfs_host: Option<String>,
    #[ts(type = "string | null")]
    pub nfs_export_root: Option<String>,
}

impl SettingsResponse {
    /// Loads all known settings from the DB in one pass. The install
    /// password is deliberately absent: it is write-only (sent as plaintext
    /// `install_password` on update, stored as a hash) and its hash is never
    /// exposed over the API.
    ///
    /// # Errors
    ///
    /// When the DB query fails.
    pub async fn load(db: &DatabaseConnection) -> ModelResult<Self> {
        Ok(Self {
            default_os_version_id: settings::Model::get_default_os_version_id(db).await?,
            install_username: settings::Model::get_install_username(db).await?,
            install_password_set: !settings::Model::get_install_password_hash(db)
                .await?
                .is_empty(),
            ssh_key: settings::Model::get_ssh_key(db).await?,
            timezone: settings::Model::get_timezone(db).await?,
            packages: settings::Model::get_packages(db).await?,
            install_disk: settings::Model::get_install_disk(db).await?,
            public_host: settings::Model::get_public_host(db).await?,
            nfs_host: settings::Model::get_nfs_host(db).await?,
            nfs_export_root: settings::Model::get_nfs_export_root(db).await?,
        })
    }
}

/// A key/value map of settings to upsert. Only the known keys are accepted.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct UpdateSettingsParams {
    pub values: std::collections::HashMap<String, String>,
}

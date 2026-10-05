use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::_entities::machines;

/// Wire representation of a machine. Timestamps are RFC3339 strings.
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct MachineResponse {
    pub id: i64,
    pub mac: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub ssh_key: Option<String>,
    pub status: String,
    pub os_version_id: Option<i64>,
    #[ts(type = "string | null")]
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    #[ts(type = "string | null")]
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[ts(type = "string | null")]
    pub install_started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[ts(type = "string | null")]
    pub installed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub notes: Option<String>,
    pub install_disk: Option<String>,
    pub cloudinit_url: Option<String>,
    pub packages: Option<String>,
    pub timezone: Option<String>,
    /// Whether the machine carries its own install password. The hash itself
    /// is never exposed — the UI only distinguishes "own" from "global".
    pub password_set: bool,
}

impl From<&machines::Model> for MachineResponse {
    fn from(machine: &machines::Model) -> Self {
        Self {
            id: machine.id,
            mac: machine.mac.clone(),
            name: machine.name.clone(),
            username: machine.username.clone(),
            ssh_key: machine.ssh_key.clone(),
            status: machine.status.clone(),
            os_version_id: machine.os_version_id,
            last_seen_at: machine.last_seen_at.map(|d| d.and_utc()),
            approved_at: machine.approved_at.map(|d| d.and_utc()),
            install_started_at: machine.install_started_at.map(|d| d.and_utc()),
            installed_at: machine.installed_at.map(|d| d.and_utc()),
            notes: machine.notes.clone(),
            install_disk: machine.install_disk.clone(),
            cloudinit_url: machine.cloudinit_url.clone(),
            packages: machine.packages.clone(),
            timezone: machine.timezone.clone(),
            password_set: machine
                .password_hash
                .as_deref()
                .map(|h| !h.trim().is_empty())
                .unwrap_or(false),
        }
    }
}

impl From<machines::Model> for MachineResponse {
    fn from(machine: machines::Model) -> Self {
        Self::from(&machine)
    }
}

/// Params of the approve transition: pick the OS version to install.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct ApproveMachineParams {
    pub os_version_id: i64,
}

/// Params of `POST /api/machines` — manual registration by MAC with an
/// optional install identity.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct CreateMachineParams {
    pub mac: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub ssh_key: Option<String>,
    pub notes: Option<String>,
    pub install_disk: Option<String>,
    pub cloudinit_url: Option<String>,
    pub packages: Option<String>,
    pub timezone: Option<String>,
    /// Write-only plaintext install password, hashed server-side.
    pub password: Option<String>,
}

/// Params of `PUT /api/machines/:id` — edit the install identity.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct UpdateMachineParams {
    pub name: Option<String>,
    pub username: Option<String>,
    pub ssh_key: Option<String>,
    pub notes: Option<String>,
    pub install_disk: Option<String>,
    pub cloudinit_url: Option<String>,
    pub packages: Option<String>,
    pub timezone: Option<String>,
    /// Write-only plaintext install password, hashed server-side.
    pub password: Option<String>,
    /// `true` clears the machine's own password (falls back to global).
    pub password_reset: Option<bool>,
}

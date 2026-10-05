use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::_entities::os_versions;

/// Wire representation of an OS version and its mirror-download progress.
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct OsVersionResponse {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub arch: String,
    pub kernel_url: String,
    pub initrd_url: String,
    pub iso_url: String,
    pub download_status: String,
    pub boot_mode: String,
    #[ts(type = "number")]
    pub bytes_downloaded: i64,
    #[ts(type = "number")]
    pub total_bytes: i64,
    pub error: Option<String>,
}

impl From<&os_versions::Model> for OsVersionResponse {
    fn from(os: &os_versions::Model) -> Self {
        Self {
            id: os.id,
            name: os.name.clone(),
            version: os.version.clone(),
            arch: os.arch.clone(),
            kernel_url: os.kernel_url.clone(),
            initrd_url: os.initrd_url.clone(),
            iso_url: os.iso_url.clone(),
            download_status: os.download_status.clone(),
            boot_mode: os.boot_mode.clone(),
            bytes_downloaded: os.bytes_downloaded,
            total_bytes: os.total_bytes,
            error: os.error.clone(),
        }
    }
}

impl From<os_versions::Model> for OsVersionResponse {
    fn from(os: os_versions::Model) -> Self {
        Self::from(&os)
    }
}

/// Params of the create endpoint.
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct CreateOsVersionParams {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub kernel_url: String,
    pub initrd_url: String,
    pub iso_url: String,
    #[serde(default = "default_boot_mode")]
    #[ts(type = "\"online\" | \"offline\" | \"nfs\"")]
    pub boot_mode: String,
}

/// Boot mode used when the client omits the field.
fn default_boot_mode() -> String {
    "online".to_string()
}

/// Params of the update endpoint (all fields optional).
#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/bindings/")]
pub struct UpdateOsVersionParams {
    pub name: Option<String>,
    pub version: Option<String>,
    pub arch: Option<String>,
    pub kernel_url: Option<String>,
    pub initrd_url: Option<String>,
    pub iso_url: Option<String>,
    #[ts(type = "\"online\" | \"offline\" | \"nfs\" | null")]
    pub boot_mode: Option<String>,
}

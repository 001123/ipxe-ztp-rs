use async_trait::async_trait;
use loco_rs::prelude::*;

use crate::data::DataDir;

/// Resolves the OS-artifact data directory at boot and publishes it on the
/// shared store.
///
/// Resolution order:
/// 1. the `settings.data_dir` override in `config/<env>.yaml`
/// 2. the platform data directory (`directories::ProjectDirs`)
///
/// The directory tree (`<data_dir>/os/`) is created eagerly so downloads and
/// file serving never race on first use.
pub struct DataDirInitializer;

#[async_trait]
impl Initializer for DataDirInitializer {
    fn name(&self) -> String {
        "data_dir".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        let data_dir = DataDir::resolve_from_config(&ctx.config)?;
        tokio::fs::create_dir_all(data_dir.os_dir()).await?;
        tracing::info!(data_dir = %data_dir.0.display(), "os artifact data dir ready");
        ctx.shared_store.insert(data_dir);
        Ok(())
    }
}

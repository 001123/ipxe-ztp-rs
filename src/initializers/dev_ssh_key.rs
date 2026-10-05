use async_trait::async_trait;
use loco_rs::prelude::*;

use crate::models::settings;

/// Seeds the global default SSH key from the environment in dev/test.
///
/// When `IPXE_ZTP_IS_TEST=true` and `IPXE_ZTP_TEST_SSH_KEY` are set (see `.env`), an existing
/// **empty** `ssh_key` setting is filled with the value at boot. The row is
/// never created here — the seed fixture owns that — so a key customized
/// through the admin Settings page always wins over the environment default.
pub struct DevSshKeyInitializer;

#[async_trait]
impl Initializer for DevSshKeyInitializer {
    fn name(&self) -> String {
        "dev_ssh_key".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        // `.env` is a dev convenience; production never sets these vars.
        dotenvy::dotenv().ok();

        let enabled = std::env::var("IPXE_ZTP_IS_TEST").is_ok_and(|v| v == "true");
        let key = std::env::var("IPXE_ZTP_TEST_SSH_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if !(enabled && key.is_some()) {
            return Ok(());
        }

        // Update only a pre-existing, still-empty row (created by the seed
        // fixture). Inserting here would collide with fixture row ids.
        match settings::Model::find_by_key(&ctx.db, settings::keys::SSH_KEY).await {
            Ok(existing) if existing.value.trim().is_empty() => {
                settings::Model::set(&ctx.db, settings::keys::SSH_KEY, key.as_deref().unwrap())
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}

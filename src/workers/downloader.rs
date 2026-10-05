use futures_util::StreamExt;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::{data::DataDir, models::os_versions};

pub struct DownloadWorker {
    pub ctx: AppContext,
}

/// Which OS version to download. Pass identifiers, not objects — the job is
/// persisted to the queue and the worker re-loads fresh state.
#[derive(Deserialize, Debug, Serialize)]
pub struct DownloadWorkerArgs {
    pub os_version_id: i64,
}

/// Progress-update interval: cumulative bytes are flushed to the DB at most
/// once every this many bytes, so chunk-sized writes do not hammer SQLite.
const PROGRESS_FLUSH_BYTES: i64 = 1024 * 1024;

#[async_trait]
impl BackgroundWorker<DownloadWorkerArgs> for DownloadWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn perform(&self, args: DownloadWorkerArgs) -> Result<()> {
        let data_dir = DataDir::from_context(&self.ctx)?;
        let os_version = os_versions::Entity::find_by_id(args.os_version_id)
            .one(&self.ctx.db)
            .await?
            .ok_or_else(|| {
                Error::Message(format!(
                    "os_version {} not found for download",
                    args.os_version_id
                ))
            })?;

        tracing::info!(os_version_id = os_version.id, name = %os_version.name, version = %os_version.version, "starting artifact download");

        let os_version = os_version
            .into_active_model()
            .begin_download(&self.ctx.db)
            .await?;

        // The ISO is the only thing fetched from the network: Ubuntu does not
        // publish casper kernel/initrd on any mirror — they only exist inside the
        // ISO — so after it lands they are extracted locally (see `crate::iso`).
        // Total is the ISO's content-length, usually known up front.
        let client = reqwest::Client::new();
        let iso_url = os_version.iso_url.clone();
        let mut known_total: i64 = 0;
        match client.head(&iso_url).send().await {
            Ok(resp) => {
                if let Some(len) = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                {
                    known_total = len;
                }
            }
            Err(e) => {
                tracing::warn!(os_version_id = os_version.id, url = %iso_url, error = %e, "HEAD probe failed; total_bytes will fill in as the download streams");
            }
        }

        let os_version = os_version
            .into_active_model()
            .set_download_progress(&self.ctx.db, 0, known_total)
            .await?;

        let dir = data_dir.ensure_os_version_dir(os_version.id).await?;

        let iso_dest = dir.join(os_version.iso_file_name());
        tracing::info!(os_version_id = os_version.id, url = %iso_url, file = %os_version.iso_file_name(), "downloading ISO");
        let downloaded = match stream_to_file(
            &client,
            &iso_url,
            &iso_dest,
            0,
            known_total,
            os_version.id,
            &self.ctx,
        )
        .await
        {
            Ok(n) => n,
            Err(e) => {
                let error = format!("download of {iso_url} failed: {e}");
                tracing::error!(os_version_id = os_version.id, error = %error);
                os_versions::ActiveModel::from(os_version.clone())
                    .mark_failed(&self.ctx.db, &error)
                    .await?;
                // The failure is durable state on the model (visible in
                // the admin UI, re-triggerable); the job itself completes.
                return Ok(());
            }
        };
        let os_version = os_versions::ActiveModel::from(os_version)
            .set_download_progress(&self.ctx.db, downloaded, known_total.max(downloaded))
            .await?;

        // Pull the boot files out of the ISO we just downloaded. vmlinuz/initrd
        // are what iPXE boots; the whole casper/ subtree is the NFS medium
        // (`netboot=nfs` mounts it directly, so low-RAM clients never copy
        // it into RAM); .disk/ is casper's medium metadata; dists/ is the
        // ISO's apt repository metadata — the live environment's /cdrom apt
        // source (`file:/cdrom`) needs dists/<suite>/Release or the
        // installer's `apt-get update` fails with exit status 100 and the
        // install aborts. pool/ (the ISO's .deb cache, ~2 GiB) is
        // deliberately not extracted: packages come from the elected
        // network mirror, and dists/ alone is enough for `apt-get update`
        // to validate the cdrom source.
        let extraction = tokio::task::spawn_blocking({
            let iso_dest = iso_dest.clone();
            let dir = dir.clone();
            move || -> std::result::Result<(), String> {
                fn extract_tree(
                    iso: &std::path::Path,
                    iso_dir: &str,
                    dest_root: &std::path::Path,
                ) -> std::result::Result<(), String> {
                    let entries = crate::iso::list_dir(iso, iso_dir)
                        .map_err(|e| format!("listing {iso_dir}/ in the iso failed: {e}"))?;
                    for (name, _entry, is_dir) in entries {
                        let iso_path = format!("{iso_dir}/{name}");
                        if is_dir {
                            extract_tree(iso, &iso_path, dest_root)?;
                            continue;
                        }
                        let dest = dest_root.join(&iso_path);
                        std::fs::create_dir_all(dest.parent().expect("dest has a parent"))
                            .map_err(|e| format!("creating {} failed: {e}", dest.display()))?;
                        crate::iso::extract_to(iso, &iso_path, &dest)
                            .map_err(|e| format!("extracting {iso_path} failed: {e}"))?;
                        // kernel+initrd are also served over HTTP from the
                        // version dir root for iPXE
                        if iso_path == "casper/vmlinuz" || iso_path == "casper/initrd" {
                            let root_dest = dest_root.join(&name);
                            std::fs::copy(&dest, &root_dest).map_err(|e| {
                                format!("copying {name} to {} failed: {e}", root_dest.display())
                            })?;
                        }
                    }
                    Ok(())
                }
                for iso_dir in ["casper", ".disk", "dists"] {
                    extract_tree(&iso_dest, iso_dir, &dir)?;
                }
                Ok(())
            }
        })
        .await;
        let extraction = match extraction {
            Ok(inner) => inner,
            Err(e) => Err(e.to_string()),
        };
        if let Err(error) = extraction {
            tracing::error!(os_version_id = os_version.id, error = %error);
            os_versions::ActiveModel::from(os_version.clone())
                .mark_failed(&self.ctx.db, &error)
                .await?;
            return Ok(());
        }

        os_versions::ActiveModel::from(os_version)
            .mark_ready(&self.ctx.db)
            .await?;
        tracing::info!(
            os_version_id = args.os_version_id,
            bytes = downloaded,
            "all artifacts downloaded"
        );

        Ok(())
    }
}

/// Streams one URL into a local file, updating the OS version's cumulative
/// progress every [`PROGRESS_FLUSH_BYTES`]. Returns the bytes written.
async fn stream_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    bytes_before: i64,
    known_total: i64,
    os_version_id: i64,
    ctx: &AppContext,
) -> Result<i64> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Message(format!("{url}: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Message(format!("{url}: {e}")))?;
    let mut file = tokio::fs::File::create(dest).await?;

    let mut stream = response.bytes_stream();
    let mut bytes_in_this_artifact: i64 = 0;
    let mut last_flushed: i64 = bytes_before;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Message(format!("{url}: {e}")))?;
        file.write_all(&chunk).await?;
        bytes_in_this_artifact += chunk.len().min(i64::MAX as usize) as i64;

        let cumulative = bytes_before + bytes_in_this_artifact;
        if cumulative - last_flushed >= PROGRESS_FLUSH_BYTES {
            last_flushed = cumulative;
            os_versions::Entity::find_by_id(os_version_id)
                .one(&ctx.db)
                .await?
                .ok_or_else(|| Error::Message("os_version vanished mid-download".to_string()))?
                .into_active_model()
                .set_download_progress(&ctx.db, cumulative, known_total.max(cumulative))
                .await?;
        }
    }
    file.flush().await?;

    Ok(bytes_in_this_artifact)
}

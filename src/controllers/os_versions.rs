use loco_rs::prelude::*;

use crate::{
    dtos::common::Page,
    models::os_versions,
    views::os_versions::{CreateOsVersionParams, OsVersionResponse, UpdateOsVersionParams},
    workers::downloader::{DownloadWorker, DownloadWorkerArgs},
};

/// `GET /api/os_versions` — paginated OS version list.
///
/// # Errors
///
/// DB failure.
#[debug_handler]
pub async fn list(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Query(pagination): Query<query::PaginationQuery>,
) -> Result<Response> {
    let page = query::paginate(&ctx.db, os_versions::Entity::find(), None, &pagination).await?;
    format::json(Page::<OsVersionResponse>::from_query(page))
}

/// `GET /api/os_versions/:id`
///
/// # Errors
///
/// Unknown id (404) or DB failure.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let os_version = os_versions::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    format::json(OsVersionResponse::from(os_version))
}

/// `POST /api/os_versions` — create an OS version pointing at its mirror
/// artifacts.
///
/// # Errors
///
/// Validation failure (400) or DB failure.
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Json(params): Json<CreateOsVersionParams>,
) -> Result<Response> {
    let create = os_versions::CreateParams {
        name: params.name,
        version: params.version,
        arch: params.arch,
        kernel_url: params.kernel_url,
        initrd_url: params.initrd_url,
        iso_url: params.iso_url,
        boot_mode: params.boot_mode,
    };
    let os_version = os_versions::ActiveModel::create(&ctx.db, &create).await?;
    tracing::info!(os_version_id = os_version.id, "os version created");
    // A fresh row starts a pending download lifecycle — kick the worker off
    // right away so the artifacts become ready without manual action.
    auto_download(&ctx, &os_version).await;
    format::json(OsVersionResponse::from(os_version))
}

/// Enqueues the mirror download for `os_version` unless one is already
/// streaming. The admin UI has no manual download button — artifacts must
/// become ready on their own — so this is the only way a download starts
/// outside tests.
///
/// `pending` is treated as orphaned (e.g. left behind by a restart): the
/// in-flight window between enqueue and `begin_download` is sub-second, so
/// re-enqueueing is safe, while `downloading` means a live worker owns the row.
async fn auto_download(ctx: &AppContext, os_version: &os_versions::Model) {
    if os_version.download_status == "downloading" {
        return;
    }
    if let Err(e) = DownloadWorker::perform_later(
        ctx,
        DownloadWorkerArgs {
            os_version_id: os_version.id,
        },
    )
    .await
    {
        tracing::error!(os_version_id = os_version.id, error = ?e, "auto-download enqueue failed");
    } else {
        tracing::info!(os_version_id = os_version.id, "download auto-enqueued");
    }
}

/// `PUT /api/os_versions/:id` — update the present fields.
///
/// # Errors
///
/// Unknown id (404), validation failure (400), or DB failure.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
    Json(params): Json<UpdateOsVersionParams>,
) -> Result<Response> {
    let existing = os_versions::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    let update = os_versions::UpdateParams {
        name: params.name,
        version: params.version,
        arch: params.arch,
        kernel_url: params.kernel_url,
        initrd_url: params.initrd_url,
        iso_url: params.iso_url,
        boot_mode: params.boot_mode,
    };
    let os_version = existing
        .into_active_model()
        .update_from_params(&ctx.db, &update)
        .await?;

    // Booting offline / nfs serves the medium from local artifacts, so
    // switching to one of those modes auto-starts the mirror download when
    // the artifacts aren't ready yet (covers failed retries and orphaned
    // pending rows). online needs no local ISO, but kernel/initrd still come
    // from the downloaded one — handled by create.
    let needs_local_artifacts = matches!(os_version.boot_mode.as_str(), "offline" | "nfs");
    if needs_local_artifacts && !os_version.artifacts_ready() {
        auto_download(&ctx, &os_version).await;
    }
    format::json(OsVersionResponse::from(os_version))
}

/// `DELETE /api/os_versions/:id`
///
/// # Errors
///
/// Unknown machine (404) or DB failure.
#[debug_handler]
pub async fn delete(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let res = os_versions::Entity::delete_by_id(id).exec(&ctx.db).await?;
    if res.rows_affected == 0 {
        return not_found();
    }
    format::empty()
}

/// `POST /api/os_versions/:id/download` — enqueue the mirror download; the
/// request returns before any byte is fetched.
///
/// # Errors
///
/// Unknown id (404) or queue failure.
#[debug_handler]
pub async fn download(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let os_version = os_versions::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    DownloadWorker::perform_later(
        &ctx,
        DownloadWorkerArgs {
            os_version_id: os_version.id,
        },
    )
    .await?;
    tracing::info!(os_version_id = os_version.id, "download enqueued");
    format::json(OsVersionResponse::from(os_version))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/os_versions")
        .add("/", get(list))
        .add("/", post(create))
        .add("/{id}", get(show))
        .add("/{id}", put(update))
        .delete("/{id}", delete)
        .add("/{id}/download", post(download))
}

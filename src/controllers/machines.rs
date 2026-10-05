use loco_rs::prelude::*;
use serde::Deserialize;
use std::str::FromStr;

use crate::{
    dtos::common::Page,
    models::{machines, os_versions},
    views::machines::{
        ApproveMachineParams, CreateMachineParams, MachineResponse, UpdateMachineParams,
    },
};

/// List query: optional status filter plus pagination.
#[derive(Debug, Deserialize)]
pub struct ListMachinesQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(flatten)]
    pub pagination: query::PaginationQuery,
}

/// `GET /api/machines` — paginated machine list, optionally filtered by
/// status.
///
/// # Errors
///
/// Invalid status filter, or DB failure.
#[debug_handler]
pub async fn list(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Query(params): Query<ListMachinesQuery>,
) -> Result<Response> {
    let status = match &params.status {
        Some(raw) => Some(machines::MachineStatus::from_str(raw)?),
        None => None,
    };
    let page = query::paginate(
        &ctx.db,
        machines::Entity::find(),
        status.map(|s| {
            model::query::condition()
                .eq(machines::Column::Status, s.as_str())
                .build()
        }),
        &params.pagination,
    )
    .await?;
    format::json(Page::<MachineResponse>::from_query(page))
}

/// `GET /api/machines/:id`
///
/// # Errors
///
/// Unknown id or DB failure (both render as 404).
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let machine = machines::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    format::json(MachineResponse::from(machine))
}

/// `POST /api/machines` — manually register a machine by MAC with an
/// optional install identity, before its first network boot.
///
/// # Errors
///
/// Validation failure (400), duplicate MAC (409), or DB failure.
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Json(params): Json<CreateMachineParams>,
) -> Result<Response> {
    let create = machines::CreateParams {
        mac: params.mac,
        name: params.name,
        username: params.username,
        ssh_key: params.ssh_key,
        notes: params.notes,
        install_disk: params.install_disk,
        cloudinit_url: params.cloudinit_url,
        timezone: params.timezone,
        password: params.password,
        packages: params.packages,
    };
    let machine = machines::ActiveModel::create(&ctx.db, &create).await?;
    format::json(MachineResponse::from(machine))
}

/// `PUT /api/machines/:id` — edit the machine's install identity (hostname,
/// username, ssh key, notes). Present fields only.
///
/// # Errors
///
/// Unknown id (404), validation failure (400), or DB failure.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
    Json(params): Json<UpdateMachineParams>,
) -> Result<Response> {
    let machine = machines::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    let update = machines::UpdateParams {
        name: params.name,
        username: params.username,
        ssh_key: params.ssh_key,
        notes: params.notes,
        install_disk: params.install_disk,
        cloudinit_url: params.cloudinit_url,
        timezone: params.timezone,
        password: params.password,
        password_reset: params.password_reset,
        packages: params.packages,
    };
    let machine = machine
        .into_active_model()
        .update_info(&ctx.db, &update)
        .await?;
    tracing::info!(machine_id = machine.id, "machine info updated");
    format::json(MachineResponse::from(machine))
}

/// `POST /api/machines/:id/approve` `{os_version_id}` — approve the machine
/// for install with the given OS version.
///
/// # Errors
///
/// Unknown machine or OS version, or DB failure.
#[debug_handler]
pub async fn approve(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
    Json(params): Json<ApproveMachineParams>,
) -> Result<Response> {
    let machine = machines::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    // the OS must exist — a dangling approval would 404 at boot time
    os_versions::Entity::find_by_id(params.os_version_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| Error::Message(format!("os_version {} not found", params.os_version_id)))?;
    let machine = machine
        .into_active_model()
        .approve(&ctx.db, params.os_version_id)
        .await?;
    tracing::info!(
        machine_id = machine.id,
        os_version_id = machine.os_version_id,
        "machine approved"
    );
    format::json(MachineResponse::from(machine))
}

/// `POST /api/machines/:id/reset` — back to `pending`, clearing the approved
/// OS.
///
/// # Errors
///
/// Unknown machine or DB failure.
#[debug_handler]
pub async fn reset(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let machine = machines::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    let machine = machine.into_active_model().reset(&ctx.db).await?;
    tracing::info!(machine_id = machine.id, "machine reset to pending");
    format::json(MachineResponse::from(machine))
}

/// `DELETE /api/machines/:id`
///
/// # Errors
///
/// Unknown machine or DB failure.
#[debug_handler]
pub async fn delete(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Path(id): Path<i64>,
) -> Result<Response> {
    let res = machines::Entity::delete_by_id(id).exec(&ctx.db).await?;
    if res.rows_affected == 0 {
        return not_found();
    }
    format::empty()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/machines")
        .add("/", get(list))
        .add("/", post(create))
        .add("/{id}", get(show))
        .add("/{id}", put(update))
        .add("/{id}/approve", post(approve))
        .add("/{id}/reset", post(reset))
        .delete("/{id}", delete)
}

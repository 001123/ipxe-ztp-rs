use loco_rs::prelude::*;

use crate::{
    models::settings,
    views::settings::{SettingsResponse, UpdateSettingsParams},
};

/// `GET /api/settings` — every known setting with defaults applied.
///
/// # Errors
///
/// DB failure.
#[debug_handler]
pub async fn show(State(ctx): State<AppContext>, _auth: auth::JWT) -> Result<Response> {
    format::json(SettingsResponse::load(&ctx.db).await?)
}

/// `PUT /api/settings` — upsert a key/value map. Keys outside the known set
/// are rejected: a typo would otherwise silently change nothing.
///
/// # Errors
///
/// Unknown key (400) or DB failure.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _auth: auth::JWT,
    Json(params): Json<UpdateSettingsParams>,
) -> Result<Response> {
    for (key, value) in &params.values {
        if !settings::keys::is_known(key) {
            return bad_request(format!("unknown setting key: {key}"));
        }
        if *key == settings::keys::INSTALL_PASSWORD {
            // write-only: hash the plaintext into the hash key instead of
            // storing it, and a blank value keeps the current password
            if !value.is_empty() {
                settings::Model::set_install_password(&ctx.db, value).await?;
            }
            continue;
        }
        if *key == settings::keys::DEFAULT_OS_VERSION_ID && value.parse::<i64>().is_err() {
            // fail early on an unparseable id rather than store garbage
            return bad_request(format!("{key} must be an integer id"));
        }
        if *key == settings::keys::PACKAGES {
            if let Err(e) = crate::models::packages::validate_list(value) {
                return bad_request(format!("{key}: {e}"));
            }
        }
        settings::Model::set(&ctx.db, key, value).await?;
    }
    format::json(SettingsResponse::load(&ctx.db).await?)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/settings")
        .add("/", get(show))
        .add("/", put(update))
}

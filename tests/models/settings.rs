use ipxe_loco_rs::{app::App, models::settings};
use loco_rs::testing::prelude::*;
use sea_orm::EntityTrait;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn accessors_apply_defaults_when_unset() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    assert_eq!(
        settings::Model::get_install_username(db).await.unwrap(),
        "ubuntu"
    );
    assert_eq!(settings::Model::get_timezone(db).await.unwrap(), "UTC");
    assert_eq!(settings::Model::get_ssh_key(db).await.unwrap(), "");
    assert_eq!(
        settings::Model::get_install_password_hash(db)
            .await
            .unwrap(),
        ""
    );
    assert_eq!(
        settings::Model::get_default_os_version_id(db)
            .await
            .unwrap(),
        None
    );
    assert_eq!(settings::Model::get_public_host(db).await.unwrap(), None);
}

#[tokio::test]
#[serial]
async fn set_then_get_roundtrip() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    settings::Model::set(db, settings::keys::TIMEZONE, "Europe/Berlin")
        .await
        .expect("insert works");
    settings::Model::set(db, settings::keys::DEFAULT_OS_VERSION_ID, "7")
        .await
        .expect("insert works");

    assert_eq!(
        settings::Model::get_timezone(db).await.unwrap(),
        "Europe/Berlin"
    );
    assert_eq!(
        settings::Model::get_default_os_version_id(db)
            .await
            .unwrap(),
        Some(7)
    );

    // set again updates the same row, not a duplicate
    settings::Model::set(db, settings::keys::TIMEZONE, "UTC")
        .await
        .expect("update works");
    assert_eq!(settings::Model::get_timezone(db).await.unwrap(), "UTC");
    let rows = settings::Entity::find().all(db).await.expect("query works");
    assert_eq!(rows.len(), 2, "upsert did not duplicate rows");
}

#[tokio::test]
#[serial]
async fn set_install_password_stores_crypt_hash() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    settings::Model::set_install_password(db, "s3cret")
        .await
        .expect("hashing + insert works");
    let hash = settings::Model::get_install_password_hash(db)
        .await
        .expect("read works");
    assert!(
        hash.starts_with("$6$"),
        "hash must be SHA-512 crypt: {hash}"
    );
    assert_ne!(hash, "s3cret", "plaintext must never be stored");

    // re-hashing updates the same row, not a duplicate
    settings::Model::set_install_password(db, "other")
        .await
        .expect("update works");
    let row = settings::Model::find_by_key(db, settings::keys::INSTALL_PASSWORD_HASH)
        .await
        .expect("row exists");
    assert!(row.value.starts_with("$6$"), "hash must be SHA-512 crypt");
}

#[tokio::test]
#[serial]
async fn unknown_keys_are_known() {
    assert!(settings::keys::is_known("timezone"));
    assert!(settings::keys::is_known(settings::keys::INSTALL_PASSWORD));
    assert!(!settings::keys::is_known("nonsense"));
}

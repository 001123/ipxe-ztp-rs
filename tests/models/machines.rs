use std::str::FromStr;

use ipxe_loco_rs::{
    app::App,
    models::{machines, os_versions},
};
use loco_rs::testing::prelude::*;
use sea_orm::IntoActiveModel;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn find_or_register_creates_pending_once() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let first = machines::Model::find_or_register_by_mac(db, "AA-BB-CC-DD-EE-01")
        .await
        .expect("first sight registers");
    assert_eq!(first.mac, "AA:BB:CC:DD:EE:01", "MAC is normalized");
    assert_eq!(first.status, "pending");

    let second = machines::Model::find_or_register_by_mac(db, "aa:bb:cc:dd:ee:01")
        .await
        .expect("second sight finds");
    assert_eq!(first.id, second.id, "same MAC, same row");
    assert_eq!(second.status, "pending", "no status churn on re-sight");

    // separators are interchangeable
    assert_eq!(
        machines::normalize_mac("AA-BB-CC-DD-EE-FF"),
        machines::normalize_mac("aa.bb.cc.dd.ee.ff")
    );
}

#[tokio::test]
#[serial]
async fn find_by_mac_misses_unknown() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    assert!(
        machines::Model::find_by_mac(db, "aa:bb:cc:dd:ee:99")
            .await
            .is_err(),
        "unknown MAC is EntityNotFound"
    );
}

#[tokio::test]
#[serial]
async fn state_transitions_lifecycle() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let machine = machines::Model::find_or_register_by_mac(db, "aa:bb:cc:dd:ee:10")
        .await
        .unwrap();

    // the FK on os_version_id is real — approve with an existing os_version
    let os_version = os_versions::ActiveModel::create(
        db,
        &os_versions::CreateParams {
            name: "Ubuntu".to_string(),
            version: "24.04".to_string(),
            arch: "amd64".to_string(),
            kernel_url: "http://m/k".to_string(),
            initrd_url: "http://m/i".to_string(),
            iso_url: "http://m/i.iso".to_string(),
            boot_mode: "online".to_string(),
        },
    )
    .await
    .unwrap();

    let approved = machine
        .clone()
        .into_active_model()
        .approve(db, os_version.id)
        .await
        .unwrap();
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.os_version_id, Some(os_version.id));
    assert!(approved.approved_at.is_some());

    let installing = approved
        .into_active_model()
        .mark_installing(db)
        .await
        .unwrap();
    assert_eq!(installing.status, "installing");

    let installed = installing
        .into_active_model()
        .mark_installed(db)
        .await
        .unwrap();
    assert_eq!(installed.status, "installed");
    assert!(installed.installed_at.is_some());

    let reset = installed.into_active_model().reset(db).await.unwrap();
    assert_eq!(reset.status, "pending");
    assert_eq!(reset.os_version_id, None);
    assert_eq!(reset.approved_at, None);
    assert_eq!(reset.installed_at, None);
}

#[tokio::test]
#[serial]
async fn status_string_roundtrip() {
    for status in machines::MachineStatus::all() {
        assert_eq!(
            machines::MachineStatus::from_str(status.as_str()).unwrap(),
            *status,
            "status parses back"
        );
    }
    assert!(machines::MachineStatus::from_str("bogus").is_err());
}

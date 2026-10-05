use ipxe_loco_rs::{
    app::App,
    models::{machines, os_versions, settings},
};
use loco_rs::testing::prelude::*;
use sea_orm::IntoActiveModel;
use serial_test::serial;

use super::prepare_data;

async fn create_os_version(ctx: &loco_rs::app::AppContext) -> os_versions::Model {
    os_versions::ActiveModel::create(
        &ctx.db,
        &os_versions::CreateParams {
            name: "Ubuntu".to_string(),
            version: "24.04".to_string(),
            arch: "amd64".to_string(),
            kernel_url: "http://archive.ubuntu.com/ubuntu/dists/noble/main/netboot/amd64/vmlinuz"
                .to_string(),
            initrd_url: "http://archive.ubuntu.com/ubuntu/dists/noble/main/netboot/amd64/initrd"
                .to_string(),
            iso_url: "http://releases.ubuntu.com/24.04/ubuntu-24.04-live-server-amd64.iso"
                .to_string(),
            boot_mode: "online".to_string(),
        },
    )
    .await
    .expect("os version fixture should insert")
}

#[tokio::test]
#[serial]
async fn admin_endpoints_require_auth() {
    request::<App, _, _>(|request, _ctx| async move {
        assert_eq!(
            request.get("/api/machines").await.status_code(),
            401,
            "machine list requires auth"
        );
        assert_eq!(
            request.get("/api/machines/1").await.status_code(),
            401,
            "machine show requires auth"
        );
        assert_eq!(
            request
                .post("/api/machines/1/approve")
                .json(&serde_json::json!({ "os_version_id": 1 }))
                .await
                .status_code(),
            401,
            "approve requires auth"
        );
        assert_eq!(
            request.post("/api/machines/1/reset").await.status_code(),
            401,
            "reset requires auth"
        );
        assert_eq!(
            request.delete("/api/machines/1").await.status_code(),
            401,
            "delete requires auth"
        );
        assert_eq!(
            request.get("/api/os_versions").await.status_code(),
            401,
            "os_versions list requires auth"
        );
        assert_eq!(
            request.get("/api/settings").await.status_code(),
            401,
            "settings require auth"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_list_machines_with_filters() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);

        machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:10")
            .await
            .unwrap();
        let other = machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:11")
            .await
            .unwrap();
        let os_version = create_os_version(&ctx).await;
        other
            .into_active_model()
            .approve(&ctx.db, os_version.id)
            .await
            .unwrap();

        let response = request
            .get("/api/machines")
            .add_header(auth_key.clone(), auth_value.clone())
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(
            body["items"].as_array().map(Vec::len),
            Some(2),
            "list returns both machines: {body}"
        );
        assert!(
            body["total_items"].is_u64(),
            "list carries the pagination metadata: {body}"
        );

        // status filter
        let response = request
            .get("/api/machines?status=approved")
            .add_header(auth_key, auth_value)
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["items"][0]["status"], "approved");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn approve_flow_transitions() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);
        let os_version = create_os_version(&ctx).await;
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:12")
            .await
            .unwrap();

        // approve
        let response = request
            .post(&format!("/api/machines/{}/approve", machine.id))
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({ "os_version_id": os_version.id }))
            .await;
        assert_eq!(response.status_code(), 200, "approve succeeds");
        let approved = machines::Model::find_by_mac(&ctx.db, &machine.mac)
            .await
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert_eq!(approved.os_version_id, Some(os_version.id));
        assert!(approved.approved_at.is_some());

        // reset
        let response = request
            .post(&format!("/api/machines/{}/reset", machine.id))
            .add_header(auth_key, auth_value)
            .await;
        assert_eq!(response.status_code(), 200);
        let reset = machines::Model::find_by_mac(&ctx.db, &machine.mac)
            .await
            .unwrap();
        assert_eq!(reset.status, "pending");
        assert_eq!(reset.os_version_id, None);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_delete_machine() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:13")
            .await
            .unwrap();

        let response = request
            .delete(&format!("/api/machines/{}", machine.id))
            .add_header(auth_key, auth_value)
            .await;
        assert_eq!(response.status_code(), 200);
        assert!(
            machines::Model::find_by_mac(&ctx.db, &machine.mac)
                .await
                .is_err(),
            "deleted machine is gone"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn os_versions_crud() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);

        // create
        let payload = serde_json::json!({
            "name": "Rocky",
            "version": "9",
            "arch": "x86_64",
            "kernel_url": "http://mirror.example/rocky/vmlinuz",
            "initrd_url": "http://mirror.example/rocky/initrd",
            "iso_url": "http://mirror.example/rocky.iso"
        });
        let response = request
            .post("/api/os_versions")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&payload)
            .await;
        assert_eq!(response.status_code(), 200, "create succeeds");
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        let id = body["id"].as_i64().expect("create returns id");
        assert_eq!(body["download_status"], "pending", "new rows start pending");

        // validation failure → 400
        let bad = serde_json::json!({
            "name": "Rocky",
            "version": "10",
            "arch": "x86_64",
            "kernel_url": "not-a-url",
            "initrd_url": "http://mirror.example/rocky/initrd",
            "iso_url": "http://mirror.example/rocky.iso"
        });
        let response = request
            .post("/api/os_versions")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&bad)
            .await;
        assert_eq!(response.status_code(), 400, "bad url is rejected");

        // update
        let response = request
            .put(&format!("/api/os_versions/{id}"))
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({ "version": "9.4" }))
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(body["version"], "9.4");

        // list
        let response = request
            .get("/api/os_versions")
            .add_header(auth_key.clone(), auth_value.clone())
            .await;
        assert_eq!(response.status_code(), 200);

        // download of a missing os version → 404 (never enqueues a download)
        let response = request
            .post("/api/os_versions/999999/download")
            .add_header(auth_key.clone(), auth_value.clone())
            .await;
        assert_eq!(response.status_code(), 404);

        // delete
        let response = request
            .delete(&format!("/api/os_versions/{id}"))
            .add_header(auth_key, auth_value)
            .await;
        assert_eq!(response.status_code(), 200);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn can_create_and_update_machine_info() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);

        // create with a normalized MAC and a full install identity
        let payload = serde_json::json!({
            "mac": "AA-BB-CC-11-22-33",
            "name": "ztp-node-1",
            "username": "ops",
            "ssh_key": "ssh-ed25519 AAAAC3 ops@host",
            "install_disk": "/dev/vdb",
            "packages": "curl\nhtop",
            "timezone": "Etc/GMT-7",
            "password": "machine-secret"
        });
        let response = request
            .post("/api/machines")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&payload)
            .await;
        assert_eq!(response.status_code(), 200, "create succeeds");
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        let id = body["id"].as_i64().expect("create returns id");
        assert_eq!(body["mac"], "AA:BB:CC:11:22:33", "MAC is normalized");
        assert_eq!(body["name"], "ztp-node-1");
        assert_eq!(body["username"], "ops");
        assert_eq!(body["ssh_key"], "ssh-ed25519 AAAAC3 ops@host");
        assert_eq!(body["install_disk"], "/dev/vdb");
        assert_eq!(body["packages"], "curl\nhtop");
        assert_eq!(body["timezone"], "Etc/GMT-7", "create stores timezone");
        assert_eq!(
            body["password_set"], true,
            "create hashes the plaintext password"
        );
        assert_eq!(body["status"], "pending", "manual rows start pending");

        // duplicate MAC → 409 conflict
        let response = request
            .post("/api/machines")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({ "mac": "aa:bb:cc:11:22:33" }))
            .await;
        assert_eq!(response.status_code(), 409, "duplicate MAC is a conflict");

        // invalid hostname → 400
        let response = request
            .post("/api/machines")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({ "mac": "aa:bb:cc:11:22:44", "name": "-bad hostname!" }))
            .await;
        assert_eq!(response.status_code(), 400, "bad hostname is rejected");

        // invalid package name → 400
        let response = request
            .post("/api/machines")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({
                "mac": "aa:bb:cc:11:22:44",
                "packages": "curl\nbad;name"
            }))
            .await;
        assert_eq!(response.status_code(), 400, "bad package name is rejected");

        // update the install identity
        let response = request
            .put(&format!("/api/machines/{id}"))
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({
                "name": "renamed-node",
                "username": "root",
                "ssh_key": "ssh-rsa KEY2",
                "install_disk": "/dev/sda",
                "cloudinit_url": "https://seed.example.com/nocloud/",
                "packages": "curl htop\nvim",
                "timezone": "Etc/GMT+5",
                "password_reset": true
            }))
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(body["name"], "renamed-node");
        assert_eq!(body["username"], "root");
        assert_eq!(body["ssh_key"], "ssh-rsa KEY2");
        assert_eq!(body["install_disk"], "/dev/sda");
        assert_eq!(body["cloudinit_url"], "https://seed.example.com/nocloud/");
        assert_eq!(body["packages"], "curl htop\nvim");
        assert_eq!(body["timezone"], "Etc/GMT+5", "update stores timezone");
        assert_eq!(
            body["password_set"], false,
            "password_reset clears the machine password"
        );

        // the lifecycle was untouched by the edit
        let updated = machines::Model::find_by_mac(&ctx.db, "aa:bb:cc:11:22:33")
            .await
            .unwrap();
        assert_eq!(updated.status, "pending");

        // unknown id → 404
        let response = request
            .put("/api/machines/999999")
            .add_header(auth_key, auth_value)
            .json(&serde_json::json!({ "name": "x" }))
            .await;
        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn settings_get_and_put() {
    request::<App, _, _>(|request, ctx| async move {
        let login = prepare_data::init_user_login(&request, &ctx).await;
        let (auth_key, auth_value) = prepare_data::auth_header(&login.token);

        // defaults before any write
        let response = request
            .get("/api/settings")
            .add_header(auth_key.clone(), auth_value.clone())
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(body["install_username"], "ubuntu", "model default applies");
        assert_eq!(body["timezone"], "UTC", "model default applies");

        // update
        let response = request
            .put("/api/settings")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({
                "values": {
                    "timezone": "Europe/Berlin",
                    "ssh_key": "ssh-ed25519 AAAAC3Nza admin@host",
                    "default_os_version_id": "7",
                    "packages": "curl\nhtop"
                }
            }))
            .await;
        assert_eq!(response.status_code(), 200);
        let body: serde_json::Value = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(body["timezone"], "Europe/Berlin");
        assert_eq!(body["ssh_key"], "ssh-ed25519 AAAAC3Nza admin@host");
        assert_eq!(body["default_os_version_id"], 7);
        assert_eq!(body["packages"], "curl\nhtop");

        // persisted
        assert_eq!(
            settings::Model::get_timezone(&ctx.db).await.unwrap(),
            "Europe/Berlin"
        );

        // unknown key → 400
        let response = request
            .put("/api/settings")
            .add_header(&auth_key, &auth_value)
            .json(&serde_json::json!({ "values": { "nonsense": "x" } }))
            .await;
        assert_eq!(response.status_code(), 400, "unknown keys are rejected");

        // invalid package name in the packages setting → 400
        let response = request
            .put("/api/settings")
            .add_header(auth_key, auth_value)
            .json(&serde_json::json!({ "values": { "packages": "curl\nbad;name" } }))
            .await;
        assert_eq!(
            response.status_code(),
            400,
            "invalid package names are rejected"
        );
    })
    .await;
}

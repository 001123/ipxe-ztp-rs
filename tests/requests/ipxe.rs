use ipxe_loco_rs::{
    app::App,
    data::DataDir,
    models::{machines, os_versions, settings},
};
use loco_rs::testing::prelude::*;
use sea_orm::IntoActiveModel;
use serial_test::serial;

/// Create an OS version usable by boot-script tests.
async fn create_os_version(ctx: &loco_rs::app::AppContext, version: &str) -> os_versions::Model {
    create_os_version_in_mode(ctx, version, os_versions::BootMode::Online).await
}

/// Create an OS version with an explicit boot mode.
async fn create_os_version_in_mode(
    ctx: &loco_rs::app::AppContext,
    version: &str,
    boot_mode: os_versions::BootMode,
) -> os_versions::Model {
    os_versions::ActiveModel::create(
        &ctx.db,
        &os_versions::CreateParams {
            name: "Ubuntu".to_string(),
            version: version.to_string(),
            arch: "amd64".to_string(),
            kernel_url: "http://archive.ubuntu.com/ubuntu/dists/noble/main/netboot/amd64/vmlinuz"
                .to_string(),
            initrd_url: "http://archive.ubuntu.com/ubuntu/dists/noble/main/netboot/amd64/initrd"
                .to_string(),
            iso_url: format!(
                "http://releases.ubuntu.com/{version}/ubuntu-{version}-live-server-amd64.iso"
            ),
            boot_mode: boot_mode.as_str().to_string(),
        },
    )
    .await
    .expect("os version fixture should insert")
}

/// Approve a machine for install with the given OS version.
async fn approve_machine(ctx: &loco_rs::app::AppContext, mac: &str, os_version_id: i64) {
    let machine = machines::Model::find_or_register_by_mac(&ctx.db, mac)
        .await
        .expect("machine fixture should exist");
    machine
        .into_active_model()
        .approve(&ctx.db, os_version_id)
        .await
        .expect("approve should persist");
}

#[tokio::test]
#[serial]
async fn boot_unknown_mac_registers_pending_and_serves_wait_loop() {
    request::<App, _, _>(|request, ctx| async move {
        let response = request.get("/ipxe/boot?mac=AA-BB-CC-DD-EE-FF").await;
        assert_eq!(response.status_code(), 200, "boot should serve a script");
        let text = response.text();
        assert!(
            text.contains("#!ipxe"),
            "script must open with #!ipxe: {text}"
        );
        assert!(
            text.contains("sleep 30"),
            "wait loop must sleep before retrying: {text}"
        );
        assert!(
            text.contains("/ipxe/boot?mac=AA:BB:CC:DD:EE:FF"),
            "wait loop must chain back into boot with the normalized MAC: {text}"
        );

        // the machine was registered, normalized
        let machine = machines::Model::find_by_mac(&ctx.db, "aa:bb:cc:dd:ee:ff")
            .await
            .expect("unknown MAC should have been registered");
        assert_eq!(machine.status, "pending");
        assert!(
            machine.last_seen_at.is_some(),
            "boot must refresh last_seen_at"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_pending_keeps_serving_wait_loop() {
    request::<App, _, _>(|request, ctx| async move {
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:01")
            .await
            .unwrap();

        let response = request
            .get(&format!("/ipxe/boot?mac={}", machine.mac))
            .await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(text.contains("sleep 30"), "pending must stay in wait loop");
        assert!(
            !text.contains("kernel"),
            "pending must not boot an installer"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, &machine.mac)
            .await
            .unwrap();
        assert_eq!(machine.status, "pending", "booting must not change status");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_approved_serves_install_script_and_marks_installing() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:02";
        let os_version = create_os_version(&ctx, "24.04").await;
        // kernel+initrd are served from the local mirror even in online mode,
        // so a bootable OS version always requires the artifacts downloaded
        let os_version = os_versions::ActiveModel::from(os_version)
            .mark_ready(&ctx.db)
            .await
            .unwrap();
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains(&format!("/ipxe/files/{}/vmlinuz", os_version.id)),
            "kernel must come from the artifact endpoint: {text}"
        );
        assert!(
            text.contains(&format!("/ipxe/files/{}/initrd", os_version.id)),
            "initrd must come from the artifact endpoint: {text}"
        );
        assert!(
            text.contains("autoinstall ds=nocloud-net;s=http://localhost:5150/ipxe/autoinstall/AA:BB:CC:00:00:02/"),
            "kernel args must point autoinstall at the nocloud seed: {text}"
        );
        assert!(
            text.contains("url=http://releases.ubuntu.com/24.04/ubuntu-24.04-live-server-amd64.iso"),
            "kernel args must carry the ISO url: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(
            machine.status, "installing",
            "serving the install script transitions to installing"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_offline_serves_iso_from_local_files() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:08";
        let os_version =
            create_os_version_in_mode(&ctx, "24.04", os_versions::BootMode::Offline).await;
        // offline boot requires a full local mirror of the artifacts
        let os_version = os_versions::ActiveModel::from(os_version)
            .mark_ready(&ctx.db)
            .await
            .unwrap();
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains(&format!(
                "url=http://localhost:5150/ipxe/files/{}/ubuntu-24.04-live-server-amd64.iso",
                os_version.id
            )),
            "offline boot must point casper at the locally served ISO: {text}"
        );
        assert!(
            !text.contains("http://releases.ubuntu.com"),
            "offline boot must not reference the remote mirror: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(machine.status, "installing");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_offline_without_ready_download_stays_approved_with_error_script() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:09";
        // offline mode, download_status left at pending
        let os_version =
            create_os_version_in_mode(&ctx, "26.04", os_versions::BootMode::Offline).await;
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("not downloaded yet"),
            "script must say the artifacts are not downloaded: {text}"
        );
        assert!(
            text.contains("sleep 30") && text.contains("/ipxe/boot?mac="),
            "script must loop back into boot instead of installing: {text}"
        );
        assert!(
            !text.contains("kernel"),
            "no installer kernel may be served: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(
            machine.status, "approved",
            "machine must stay approved until artifacts are ready"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_online_without_ready_download_stays_approved_with_error_script() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:0c";
        // online mode, download_status left at pending: the install script
        // would fetch kernel/initrd from the local mirror and 404, leaving
        // the machine stuck in `installing` booting a blank disk forever
        let os_version = create_os_version(&ctx, "26.04").await;
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("not downloaded yet"),
            "online boot must also wait for the local kernel/initrd: {text}"
        );
        assert!(
            !text.contains("kernel"),
            "no installer kernel may be served: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(
            machine.status, "approved",
            "machine must stay approved until artifacts are ready"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_nfs_serves_nfsroot_and_no_iso_url() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:0d";
        let os_version = create_os_version_in_mode(&ctx, "24.04", os_versions::BootMode::Nfs).await;
        let os_version = os_versions::ActiveModel::from(os_version)
            .mark_ready(&ctx.db)
            .await
            .unwrap();
        // nfs readiness requires the extracted casper medium on disk
        let data_dir = DataDir::from_context(&ctx).expect("data dir initialized");
        let dir = data_dir
            .ensure_os_version_dir(os_version.id)
            .await
            .expect("artifact dir");
        tokio::fs::create_dir_all(dir.join("casper"))
            .await
            .expect("casper dir");
        tokio::fs::write(dir.join("casper/filesystem.squashfs"), b"x")
            .await
            .expect("casper medium");
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("netboot=nfs nfsroot="),
            "nfs boot must mount the medium over nfs: {text}"
        );
        assert!(
            !text.contains("netboot=url"),
            "nfs boot must not copy the iso into ram: {text}"
        );
        assert!(
            text.contains(&format!("/ipxe/files/{}/vmlinuz", os_version.id)),
            "kernel still comes from the local mirror: {text}"
        );
        assert!(
            text.contains("autoinstall ds=nocloud-net"),
            "the autoinstall seed arg is kept: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(machine.status, "installing");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_nfs_without_casper_medium_stays_approved_with_error_script() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:0e";
        // artifacts "ready" but the casper dir was never extracted
        let os_version = create_os_version_in_mode(&ctx, "24.04", os_versions::BootMode::Nfs).await;
        let os_version = os_versions::ActiveModel::from(os_version)
            .mark_ready(&ctx.db)
            .await
            .unwrap();
        // the test data dir persists across runs: drop any casper medium a
        // previous boot_nfs run left for the same os version id
        let data_dir = DataDir::from_context(&ctx).expect("data dir initialized");
        let dir = data_dir
            .ensure_os_version_dir(os_version.id)
            .await
            .expect("artifact dir");
        tokio::fs::remove_dir_all(dir.join("casper")).await.ok();
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            !text.contains("kernel"),
            "no installer kernel may be served without the nfs medium: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(
            machine.status, "approved",
            "machine must stay approved until the nfs medium is extracted"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_installing_and_installed_boot_local_disk() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:03";
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, mac)
            .await
            .unwrap();
        let machine = machine
            .into_active_model()
            .mark_installing(&ctx.db)
            .await
            .unwrap();

        let response = request
            .get(&format!("/ipxe/boot?mac={}", machine.mac))
            .await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("sanboot --no-describe --drive 0x80") && !text.contains("exit"),
            "installing machines boot from local disk: sanboot, no EFI_SUCCESS exit"
        );

        let machine = machine
            .into_active_model()
            .mark_installed(&ctx.db)
            .await
            .unwrap();
        let response = request
            .get(&format!("/ipxe/boot?mac={}", machine.mac))
            .await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("sanboot --no-describe --drive 0x80") && !text.contains("exit"),
            "installed machines boot from local disk: sanboot, no EFI_SUCCESS exit"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn installed_callback_marks_installed() {
    request::<App, _, _>(|request, ctx| async move {
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, "aa:bb:cc:00:00:04")
            .await
            .unwrap();

        let response = request
            .get(&format!("/ipxe/installed/{}", machine.mac))
            .await;
        assert_eq!(response.status_code(), 200);

        let machine = machines::Model::find_by_mac(&ctx.db, &machine.mac)
            .await
            .unwrap();
        assert_eq!(machine.status, "installed");
        assert!(machine.installed_at.is_some());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn autoinstall_renders_settings() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:05";
        machines::Model::find_or_register_by_mac(&ctx.db, mac)
            .await
            .unwrap();
        settings::Model::set(&ctx.db, settings::keys::INSTALL_USERNAME, "deployer")
            .await
            .unwrap();
        settings::Model::set(
            &ctx.db,
            settings::keys::SSH_KEY,
            "ssh-ed25519 AAAAC3Nza test@host",
        )
        .await
        .unwrap();
        settings::Model::set(&ctx.db, settings::keys::TIMEZONE, "Europe/Paris")
            .await
            .unwrap();

        let response = request
            .get(&format!("/ipxe/autoinstall/{mac}/user-data"))
            .await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("#cloud-config"),
            "user-data opens with #cloud-config: {text}"
        );
        assert!(
            text.contains("username: deployer"),
            "username comes from settings: {text}"
        );
        assert!(
            text.contains("ssh-ed25519 AAAAC3Nza test@host"),
            "ssh key comes from settings: {text}"
        );
        assert!(
            text.contains("timezone: Europe/Paris"),
            "timezone comes from settings: {text}"
        );
        assert!(
            text.contains(&format!(
                "wget -qO- http://localhost:5150/ipxe/installed/{}",
                mac.to_uppercase()
            )),
            "late-command must call back the installed endpoint: {text}"
        );

        let meta = request
            .get(&format!("/ipxe/autoinstall/{mac}/meta-data"))
            .await;
        assert_eq!(meta.status_code(), 200);
        let meta_text = meta.text();
        assert!(meta_text.contains(&format!("instance-id: {}", mac.to_uppercase())));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn autoinstall_per_machine_identity_overrides_settings() {
    request::<App, _, _>(|request, ctx| async move {
        // global defaults
        settings::Model::set(&ctx.db, settings::keys::INSTALL_USERNAME, "default-user")
            .await
            .unwrap();
        settings::Model::set(&ctx.db, settings::keys::SSH_KEY, "ssh-ed25519 default-key")
            .await
            .unwrap();
        settings::Model::set(&ctx.db, settings::keys::INSTALL_DISK, "/dev/vda")
            .await
            .unwrap();
        settings::Model::set_install_password(&ctx.db, "global-secret")
            .await
            .unwrap();
        let settings_hash = settings::Model::get_install_password_hash(&ctx.db)
            .await
            .unwrap();

        // machine with its own identity
        let mac = "aa:bb:cc:00:00:06";
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, mac)
            .await
            .unwrap();
        machine
            .into_active_model()
            .update_info(
                &ctx.db,
                &machines::UpdateParams {
                    name: Some("node-a".to_string()),
                    username: Some("custom-user".to_string()),
                    ssh_key: Some("ssh-ed25519 custom-key".to_string()),
                    notes: None,
                    install_disk: Some("/dev/sda".to_string()),
                    cloudinit_url: None,
                    timezone: Some("Pacific/Auckland".to_string()),
                    password: Some("machine-secret".to_string()),
                    password_reset: None,
                    packages: None,
                },
            )
            .await
            .unwrap();

        let text = request
            .get(&format!("/ipxe/autoinstall/{mac}/user-data"))
            .await
            .text();
        assert!(
            text.contains("hostname: node-a"),
            "per-machine hostname wins: {text}"
        );
        assert!(
            text.contains("username: custom-user"),
            "per-machine username wins: {text}"
        );
        assert!(
            text.contains("timezone: Pacific/Auckland"),
            "per-machine timezone wins: {text}"
        );
        assert!(
            text.contains("password: '$6$"),
            "per-machine password is hashed SHA-512 crypt: {text}"
        );
        assert!(
            !text.contains(&settings_hash),
            "the settings hash must not leak into an overridden machine: {text}"
        );
        assert!(
            text.contains("ssh-ed25519 custom-key"),
            "per-machine ssh key wins: {text}"
        );
        assert!(
            !text.contains("default-user") && !text.contains("default-key"),
            "settings defaults must not leak into an overridden machine: {text}"
        );
        assert!(
            text.contains("  storage:\n    version: 2\n    config:\n      - {id: disk0, type: disk, path: /dev/sda"),
            "pinned install disk renders a storage config: {text}"
        );
        assert!(
            text.contains("path: /boot/efi"),
            "storage config mounts the ESP: {text}"
        );

        let meta = request
            .get(&format!("/ipxe/autoinstall/{mac}/meta-data"))
            .await
            .text();
        assert!(
            meta.contains("local-hostname: node-a"),
            "meta-data carries the per-machine hostname: {meta}"
        );

        // machine without its own identity falls back to the settings
        let mac2 = "aa:bb:cc:00:00:07";
        machines::Model::find_or_register_by_mac(&ctx.db, mac2)
            .await
            .unwrap();
        let text2 = request
            .get(&format!("/ipxe/autoinstall/{mac2}/user-data"))
            .await
            .text();
        assert!(
            text2.contains("hostname: AABBCC000007"),
            "MAC-derived hostname fallback: {text2}"
        );
        assert!(
            text2.contains("username: default-user"),
            "settings username fallback: {text2}"
        );
        assert!(
            text2.contains("ssh-ed25519 default-key"),
            "settings ssh key fallback: {text2}"
        );
        assert!(
            text2.contains(&settings_hash),
            "settings password hash fallback: {text2}"
        );
        assert!(
            text2.contains("path: /dev/vda"),
            "settings install disk fallback pins the storage config: {text2}"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn autoinstall_renders_packages_with_machine_override() {
    request::<App, _, _>(|request, ctx| async move {
        // global default packages
        settings::Model::set(&ctx.db, settings::keys::PACKAGES, "curl\nhtop")
            .await
            .unwrap();

        // machine with its own package list wins over the global default
        let mac = "aa:bb:cc:00:00:1a";
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, mac)
            .await
            .unwrap();
        machine
            .into_active_model()
            .update_info(
                &ctx.db,
                &machines::UpdateParams {
                    name: None,
                    username: None,
                    ssh_key: None,
                    notes: None,
                    install_disk: None,
                    cloudinit_url: None,
                    timezone: None,
                    password: None,
                    password_reset: None,
                    packages: Some("vim\njq".to_string()),
                },
            )
            .await
            .unwrap();

        let text = request
            .get(&format!("/ipxe/autoinstall/{mac}/user-data"))
            .await
            .text();
        assert!(
            text.contains("  packages:\n    - vim\n    - jq\n"),
            "per-machine packages win: {text}"
        );
        assert!(
            !text.contains("- htop"),
            "the global default must not leak into an overridden machine: {text}"
        );

        // a machine without its own list falls back to the global default
        let mac2 = "aa:bb:cc:00:00:1b";
        machines::Model::find_or_register_by_mac(&ctx.db, mac2)
            .await
            .unwrap();
        let text2 = request
            .get(&format!("/ipxe/autoinstall/{mac2}/user-data"))
            .await
            .text();
        assert!(
            text2.contains("  packages:\n    - curl\n    - htop\n"),
            "global default packages apply: {text2}"
        );

        // with no packages anywhere the section is absent entirely
        let mac3 = "aa:bb:cc:00:00:1c";
        machines::Model::find_or_register_by_mac(&ctx.db, mac3)
            .await
            .unwrap();
        settings::Model::set(&ctx.db, settings::keys::PACKAGES, "")
            .await
            .unwrap();
        let text3 = request
            .get(&format!("/ipxe/autoinstall/{mac3}/user-data"))
            .await
            .text();
        assert!(
            !text3.contains("packages:"),
            "no packages configured means no packages section: {text3}"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_uses_machine_cloudinit_url_as_seed() {
    request::<App, _, _>(|request, ctx| async move {
        let mac = "aa:bb:cc:00:00:0a";
        let os_version = create_os_version(&ctx, "24.04").await;
        let os_version = os_versions::ActiveModel::from(os_version)
            .mark_ready(&ctx.db)
            .await
            .unwrap();
        let machine = machines::Model::find_or_register_by_mac(&ctx.db, mac)
            .await
            .unwrap();
        machine
            .into_active_model()
            .update_info(
                &ctx.db,
                &machines::UpdateParams {
                    name: None,
                    username: None,
                    ssh_key: None,
                    notes: None,
                    install_disk: None,
                    cloudinit_url: Some("https://seed.example.com/nocloud/".to_string()),
                    timezone: None,
                    password: None,
                    password_reset: None,
                    packages: None,
                },
            )
            .await
            .unwrap();
        approve_machine(&ctx, mac, os_version.id).await;

        let response = request.get(&format!("/ipxe/boot?mac={mac}")).await;
        assert_eq!(response.status_code(), 200);
        let text = response.text();
        assert!(
            text.contains("ds=nocloud-net;s=https://seed.example.com/nocloud/"),
            "the machine's own cloud-init seed must be used: {text}"
        );
        assert!(
            !text.contains("/ipxe/autoinstall/"),
            "the server seed must not leak in when a cloud-init URL is set: {text}"
        );

        let machine = machines::Model::find_by_mac(&ctx.db, mac).await.unwrap();
        assert_eq!(machine.status, "installing");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn autoinstall_unknown_mac_is_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .get("/ipxe/autoinstall/aa:bb:cc:99:99:99/user-data")
            .await;
        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn boot_without_mac_is_400() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/ipxe/boot").await;
        assert_eq!(response.status_code(), 400);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn files_serves_downloaded_artifacts() {
    request::<App, _, _>(|request, ctx| async move {
        let os_version = create_os_version(&ctx, "26.04").await;
        let data_dir = DataDir::from_context(&ctx).expect("data dir initialized");
        let dir = data_dir
            .ensure_os_version_dir(os_version.id)
            .await
            .expect("artifact dir");
        tokio::fs::write(dir.join("vmlinuz"), b"FAKE-KERNEL-BYTES")
            .await
            .expect("write fake kernel");

        let response = request
            .get(&format!("/ipxe/files/{}/vmlinuz", os_version.id))
            .await;
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.text(), "FAKE-KERNEL-BYTES");

        // an artifact that was never downloaded is a 404, not a 500
        let response = request
            .get(&format!("/ipxe/files/{}/initrd", os_version.id))
            .await;
        assert_eq!(response.status_code(), 404);

        // path traversal is rejected
        let response = request
            .get(&format!("/ipxe/files/{}/..%2Fsecrets", os_version.id))
            .await;
        assert_ne!(response.status_code(), 200);
    })
    .await;
}

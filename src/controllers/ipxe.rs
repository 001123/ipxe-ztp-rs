use axum::body::Body;
use loco_rs::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;

use crate::{
    data::DataDir,
    models::{machines, os_versions, packages, settings},
};

/// Query params of the boot endpoint.
#[derive(Debug, Deserialize)]
pub struct BootParams {
    #[serde(default)]
    pub mac: Option<String>,
}

/// Resolve the base URL iPXE clients should use to reach this server, in
/// priority order: the `public_host` DB setting (admin UI), then the
/// `settings.public_host` config value (env-interpolated), then
/// `server.host:port` from config.
///
/// # Errors
///
/// When the DB query fails.
pub async fn public_host(ctx: &AppContext) -> Result<String> {
    if let Some(host) = settings::Model::get_public_host(&ctx.db).await? {
        return Ok(host.trim_end_matches('/').to_string());
    }
    if let Some(cfg) = &ctx.config.settings {
        if let Some(host) = cfg.get("public_host").and_then(serde_json::Value::as_str) {
            let host = host.trim();
            if !host.is_empty() {
                return Ok(host.trim_end_matches('/').to_string());
            }
        }
    }
    Ok(format!(
        "{}:{}",
        ctx.config.server.host.trim_end_matches('/'),
        ctx.config.server.port
    ))
}

/// The "waiting for approval" loop: tell the operator what is happening, wait,
/// and chain back into the boot script so the machine picks up its new state.
#[must_use]
pub fn waiting_script(base_url: &str, mac: &str) -> String {
    format!(
        "#!ipxe\n\
         echo Machine {mac} is waiting for approval on the ZTP server...\n\
         sleep 30\n\
         chain {base_url}/ipxe/boot?mac={mac}\n"
    )
}

/// Resolve the ISO URL the installer's casper should fetch from, honoring
/// the OS version's boot mode: `online` hands out the mirror URL as-is,
/// `offline` points at the locally downloaded copy served by this server.
#[must_use]
pub fn iso_url_for(base_url: &str, os_version: &os_versions::Model) -> String {
    match os_versions::BootMode::from_str(&os_version.boot_mode) {
        Ok(os_versions::BootMode::Offline) => format!(
            "{base_url}/ipxe/files/{}/{}",
            os_version.id,
            os_version.iso_file_name(),
        ),
        _ => os_version.iso_url.clone(),
    }
}

/// The install script: pull kernel+initrd from the local mirror of the
/// artifacts and point autoinstall at the machine's cloud-init seed — its
/// own URL when set, otherwise the nocloud seed this server serves. The
/// casper medium depends on the boot mode: online/offline casper downloads
/// the ISO into the client's RAM (`url=`), nfs mounts the extracted casper
/// directory over NFS (`netboot=nfs nfsroot=`) so low-RAM clients never
/// copy the medium into RAM.
#[must_use]
pub fn install_script(
    base_url: &str,
    mac: &str,
    os_version: &os_versions::Model,
    cloudinit_url: Option<&str>,
    nfs_root: Option<&str>,
) -> String {
    let seed = cloudinit_url
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map_or_else(
            || format!("{base_url}/ipxe/autoinstall/{mac}/"),
            str::to_string,
        );
    let medium = match nfs_root {
        Some(root) => format!("netboot=nfs nfsroot={root}"),
        None => format!("netboot=url url={}", iso_url_for(base_url, os_version)),
    };
    // iPXE passes kernel arguments verbatim: the `;` in `ds=nocloud-net;s=`
    // needs NO backslash escape (that is a GRUB-ism — the escaped form
    // reaches the kernel literally and breaks cloud-init's datasource
    // parsing, making subiquity fall back to the interactive installer).
    format!(
        "#!ipxe\n\
         kernel {base_url}/ipxe/files/{id}/vmlinuz ip=dhcp boot=casper {medium} \
         autoinstall ds=nocloud-net;s={seed}\n\
         initrd {base_url}/ipxe/files/{id}/initrd\n\
         boot\n",
        id = os_version.id,
    )
}

/// Resolve the `nfsroot=` value for NFS boot mode: the client-facing host
/// (explicit `nfs_host` setting, else the host part of the public host) and
/// the exported path mirroring the data dir (explicit `nfs_export_root`
/// setting, else the data dir itself). The exported path must contain
/// `os/<id>/casper/` — see the README's NFS setup section.
///
/// # Errors
///
/// When the DB query fails.
pub async fn nfs_root(ctx: &AppContext, os_version_id: i64) -> Result<String> {
    let data_dir = DataDir::from_context(ctx)?;
    let export_root = match settings::Model::get_nfs_export_root(&ctx.db).await? {
        Some(root) => PathBuf::from(root.trim_end_matches('/')),
        // the export mirrors the data dir; canonicalize so the client gets
        // an absolute host path instead of a possibly relative config value
        None => data_dir
            .canonicalize()
            .unwrap_or_else(|_| data_dir.0.clone()),
    };
    let host = match settings::Model::get_nfs_host(&ctx.db).await? {
        Some(host) => host,
        None => public_host(ctx)
            .await?
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(':')
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string(),
    };
    Ok(format!(
        "{host}:{}/os/{os_version_id}",
        export_root.to_string_lossy().trim_end_matches('/')
    ))
}

/// Artifacts not downloaded yet: tell the operator loudly and loop back into
/// the boot endpoint instead of handing out a script whose kernel/initrd
/// fetch would 404 and leave the machine stuck in `installing`.
#[must_use]
pub fn artifacts_not_ready_script(base_url: &str, mac: &str, os: &os_versions::Model) -> String {
    format!(
        "#!ipxe\n\
         echo The artifacts of {} {} are not downloaded yet.\n\
         echo Download them in the ZTP server UI, then reboot this machine.\n\
         sleep 30\n\
         chain {base_url}/ipxe/boot?mac={mac}\n",
        os.name, os.version,
    )
}

/// Installed machines boot from their own disk, never from the network.
///
/// The script is a bare `sanboot --drive 0x80` with no `exit`. On BIOS,
/// `0x80` is the int-13h boot disk. On UEFI, iPXE's SAN boot runs the disk
/// through the EFI fallback loader (`\EFI\BOOT\BOOTX64.EFI` on the ESP —
/// verified on the Proxmox OVMF + ipxe.efi setup: serial shows "Booting
/// from SAN device 0x80", then grub takes over and the installed Ubuntu
/// boots). Should a build lack that path, the failed script makes iPXE quit
/// with an EFI *error* status (script failure propagates: `script_exec` →
/// `autoboot()` → `main()` → `_efi_start` returns `EFIRC(rc)`), and the
/// UEFI boot manager keeps walking the remaining Boot#### options on
/// error — the local disk gets booted by the firmware itself. Either way,
/// net-first boot order is safe for the machine's whole lifetime: an
/// installed machine reaches its disk on every reboot, and a wiped disk
/// lands back in iPXE for re-provisioning (see `utils/proxmox/create-vm.sh`).
///
/// The previous form (`iseq ${platform} efi && exit || sanboot …`) was the
/// bug: `exit` returns `EFI_SUCCESS`, and this boot manager stops on
/// success instead of trying the next option — every reboot of an installed
/// machine then ended on the firmware front page (a black screen).
#[must_use]
pub fn local_disk_script() -> &'static str {
    "#!ipxe\nsanboot --no-describe --drive 0x80\n"
}

/// `GET /ipxe/boot?mac=` — the single URL dnsmasq points at. Serves an iPXE
/// script shaped by the machine's lifecycle state and always refreshes
/// `last_seen_at`.
///
/// # Errors
///
/// When the DB fails, or `mac` is missing.
#[debug_handler]
pub async fn boot(
    State(ctx): State<AppContext>,
    Query(params): Query<BootParams>,
) -> Result<Response> {
    let Some(mac) = params.mac.filter(|m| !m.trim().is_empty()) else {
        return bad_request("mac query parameter is required");
    };

    let machine = machines::Model::find_or_register_by_mac(&ctx.db, &mac).await?;
    let machine = machine.into_active_model().touch_last_seen(&ctx.db).await?;

    match machines::MachineStatus::from_str(&machine.status)? {
        // approved → hand out the install script and flip to installing,
        // unless offline boot was chosen while the local artifacts are not
        // downloaded yet (then loop loudly, machine stays approved)
        machines::MachineStatus::Approved => {
            let os_version_id = machine.os_version_id.ok_or_else(|| {
                Error::Message(format!(
                    "machine {} is approved but has no os_version",
                    machine.mac
                ))
            })?;
            let os_version = os_versions::Entity::find_by_id(os_version_id)
                .one(&ctx.db)
                .await?
                .ok_or_else(|| {
                    Error::Message(format!(
                        "os_version {os_version_id} of machine {} is missing",
                        machine.mac
                    ))
                })?;
            let base_url = public_host(&ctx).await?;
            let mac = machine.mac.clone();
            let cloudinit_url = machine.cloudinit_url.clone();
            let mode = os_versions::BootMode::from_str(&os_version.boot_mode).unwrap_or_default();
            // kernel+initrd are always served from the local artifact mirror,
            // regardless of boot mode — so the artifacts must be downloaded
            // even for online boots, or the install script would 404 at boot
            // and leave the machine stuck in `installing` forever. NFS boot
            // additionally needs the extracted casper medium on disk.
            let nfs_medium_ready = mode == os_versions::BootMode::Nfs && {
                let casper_dir = os_version.nfs_medium_dir(&DataDir::from_context(&ctx)?.0);
                let mut has_squashfs = false;
                if let Ok(mut entries) = tokio::fs::read_dir(&casper_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if entry.file_name().to_string_lossy().ends_with(".squashfs") {
                            has_squashfs = true;
                            break;
                        }
                    }
                }
                has_squashfs
            };
            let ready = os_version.artifacts_ready()
                && (mode != os_versions::BootMode::Nfs || nfs_medium_ready);
            let script = if ready {
                let nfs_root = match mode {
                    os_versions::BootMode::Nfs => Some(nfs_root(&ctx, os_version.id).await?),
                    _ => None,
                };
                machine.into_active_model().mark_installing(&ctx.db).await?;
                install_script(
                    &base_url,
                    &mac,
                    &os_version,
                    cloudinit_url.as_deref(),
                    nfs_root.as_deref(),
                )
            } else {
                artifacts_not_ready_script(&base_url, &mac, &os_version)
            };
            format::text(&script)
        }
        // installing/installed → boot from local disk
        machines::MachineStatus::Installing | machines::MachineStatus::Installed => {
            format::text(local_disk_script())
        }
        // pending/failed (and any freshly registered MAC) → wait loop
        machines::MachineStatus::Pending | machines::MachineStatus::Failed => {
            let script = waiting_script(&public_host(&ctx).await?, &machine.mac);
            format::text(&script)
        }
    }
}

/// The curtin storage config pinning the install to the given disk (GPT with
/// an ESP plus a root partition filling the rest). Autoinstall's default —
/// the largest disk — applies when no disk is pinned, so the caller omits
/// the section entirely in that case.
#[must_use]
pub fn storage_config(install_disk: &str) -> String {
    let disk = install_disk.trim();
    format!(
        "  storage:\n\
         \x20   version: 2\n\
         \x20   config:\n\
         \x20     - {{id: disk0, type: disk, path: {disk}, ptable: gpt, wipe: superblock-recursive, grub_device: true, preserve: false, name: \"\"}}\n\
         \x20     - {{id: esp-part, type: partition, device: disk0, size: 536870912, flag: boot, grub_device: true, preserve: false}}\n\
         \x20     - {{id: root-part, type: partition, device: disk0, size: -1, preserve: false}}\n\
         \x20     - {{id: esp-fs, type: format, volume: esp-part, fstype: fat32}}\n\
         \x20     - {{id: root-fs, type: format, volume: root-part, fstype: ext4}}\n\
         \x20     - {{id: root-mnt, type: mount, device: root-fs, path: /}}\n\
         \x20     - {{id: esp-mnt, type: mount, device: esp-fs, path: /boot/efi}}\n"
    )
}

/// `GET /ipxe/autoinstall/{mac}/user-data` — the cloud-init seed for the
/// autoinstall, rendered from the settings.
///
/// # Errors
///
/// When the machine is unknown or the DB fails.
#[debug_handler]
pub async fn autoinstall_userdata(
    State(ctx): State<AppContext>,
    Path(mac): Path<String>,
) -> Result<Response> {
    let machine = machines::Model::find_by_mac(&ctx.db, &mac).await?;
    let settings_username = settings::Model::get_install_username(&ctx.db).await?;
    let settings_password_hash = settings::Model::get_install_password_hash(&ctx.db).await?;
    let settings_ssh_key = settings::Model::get_ssh_key(&ctx.db).await?;
    let settings_timezone = settings::Model::get_timezone(&ctx.db).await?;
    let base_url = public_host(&ctx).await?;

    // per-machine identity wins; fall back to the global settings
    let mac_hostname = machine.mac.replace(':', "");
    let hostname = machine
        .name
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .unwrap_or(&mac_hostname)
        .to_string();
    let username = machine
        .username
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map_or_else(|| settings_username, str::to_string);
    let ssh_key = machine
        .ssh_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map_or_else(|| settings_ssh_key, str::to_string);
    // timezone of the installed system: the machine's own choice wins; unset
    // falls back to the global settings (which itself defaults to UTC)
    let timezone = machine
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map_or_else(|| settings_timezone, str::to_string);
    // install password: the machine's own hash wins; unset falls back to the
    // global settings hash (which may be empty — a locked account then)
    let password_hash = machine
        .password_hash
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map_or_else(|| settings_password_hash, str::to_string);
    // packages installed right after the OS install (autoinstall's `packages:`
    // section): the machine's own list wins; an empty machine list falls back
    // to the global default
    let packages_raw = match machine
        .packages
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(raw) => raw.to_string(),
        // machine list unset → fall back to the global default
        None => settings::Model::get_packages(&ctx.db).await?,
    };
    let package_list = packages::normalize_list(&packages_raw);

    let mut user_data = format!(
        "#cloud-config\n\
         autoinstall:\n\
         \x20 version: 1\n\
         \x20 shutdown: reboot\n\
         \x20 identity:\n\
         \x20   hostname: {hostname}\n\
         \x20   username: {username}\n\
         \x20   password: '{password_hash}'\n"
    );
    if !ssh_key.trim().is_empty() {
        user_data.push_str(&format!(
            "  ssh:\n\
             \x20   install-server: true\n\
             \x20   authorized-keys:\n\
             \x20     - {ssh_key}\n"
        ));
    }
    user_data.push_str(&format!(
        "  timezone: {timezone}\n\
         \x20 late-commands:\n\
         \x20   - wget -qO- {base_url}/ipxe/installed/{}\n",
        machine.mac
    ));
    // post-install package list; subiquity runs apt in-target for these
    // right after the OS itself is installed, before late-commands
    if !package_list.is_empty() {
        user_data.push_str("  packages:\n");
        for package in &package_list {
            user_data.push_str(&format!("    - {package}\n"));
        }
    }
    // deterministic DHCP networking: with netboot the initramfs holds the
    // NIC configuration and cloud-init's fallback (our network-config seed
    // is intentionally not served) can leave DNS half-configured, which
    // makes in-target `apt-get update` fail — spell the netplan out
    user_data.push_str(
        "  network:\n\
         \x20   network:\n\
         \x20     version: 2\n\
         \x20     ethernets:\n\
         \x20       all-nics:\n\
         \x20         match:\n\
         \x20           name: \"e*\"\n\
         \x20         dhcp4: true\n",
    );
    // pin the install disk: the machine's own disk wins; unset falls back to
    // the global settings disk, and with neither autoinstall applies its
    // default layout (largest disk)
    let settings_disk = settings::Model::get_install_disk(&ctx.db).await?;
    let install_disk = machine
        .install_disk
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .or(settings_disk);
    if let Some(disk) = install_disk {
        user_data.push_str(&storage_config(&disk));
    }

    format::text(&user_data)
}

/// `GET /ipxe/autoinstall/{mac}/meta-data` — the nocloud meta-data seed.
///
/// # Errors
///
/// When the machine is unknown or the DB fails.
#[debug_handler]
pub async fn autoinstall_metadata(
    State(ctx): State<AppContext>,
    Path(mac): Path<String>,
) -> Result<Response> {
    let machine = machines::Model::find_by_mac(&ctx.db, &mac).await?;
    // per-machine hostname when set, otherwise the MAC-derived default
    let mac_hostname = machine.mac.replace(':', "");
    let hostname = machine
        .name
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .unwrap_or(&mac_hostname);
    format::text(&format!(
        "instance-id: {}\nlocal-hostname: {}\n",
        machine.mac, hostname
    ))
}

/// `GET /ipxe/installed/{mac}` — called by the installer's late-command; flips
/// the machine to `installed`.
///
/// # Errors
///
/// When the machine is unknown or the DB fails.
#[debug_handler]
pub async fn installed(State(ctx): State<AppContext>, Path(mac): Path<String>) -> Result<Response> {
    let machine = machines::Model::find_by_mac(&ctx.db, &mac).await?;
    let machine = machine.into_active_model().mark_installed(&ctx.db).await?;
    tracing::info!(mac = %machine.mac, "machine reported install complete");
    format::text("ok\n")
}

/// `GET /ipxe/files/{os_version_id}/{file}` — serve a downloaded artifact
/// (kernel, initrd, ISO) from the data dir.
///
/// # Errors
///
/// When the OS version is unknown, the file name is not a bare name, or the
/// artifact has not been downloaded.
#[debug_handler]
pub async fn serve_file(
    State(ctx): State<AppContext>,
    Path((os_version_id, file)): Path<(i64, String)>,
) -> Result<Response> {
    let os_version = os_versions::Entity::find_by_id(os_version_id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| ModelError::EntityNotFound)?;
    let data_dir = DataDir::from_context(&ctx)?;

    let path = os_version
        .artifact_path(&data_dir.0, &file)
        .ok_or_else(|| ModelError::EntityNotFound)?;

    let file = tokio::fs::File::open(&path).await.map_err(|_| {
        // an artifact that is not on disk simply is not there yet
        ModelError::EntityNotFound
    })?;
    let len = file.metadata().await?.len();

    // vmlinuz/initrd/ISO have no useful media type; iPXE only needs the bytes.
    // Stream from disk instead of reading the whole file into memory: an ISO
    // is multiple GiB and would be copied fully into RAM per boot otherwise.
    let stream = tokio_util::io::ReaderStream::with_capacity(file, 64 * 1024);
    format::render()
        .header("content-type", "application/octet-stream")
        .header("content-length", len.to_string())
        .response()
        .body(Body::from_stream(stream))
        .map_err(|e| Error::Message(e.to_string()))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/ipxe")
        .add("/boot", get(boot))
        .add("/autoinstall/{mac}/user-data", get(autoinstall_userdata))
        .add("/autoinstall/{mac}/meta-data", get(autoinstall_metadata))
        .add("/installed/{mac}", get(installed))
        .add("/files/{os_version_id}/{file}", get(serve_file))
}

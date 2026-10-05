//! Re-extract the casper medium and apt metadata from an already-downloaded
//! ISO without re-downloading it — e.g. after a downloader change, or when
//! switching an OS version to NFS boot mode on an existing install.
//!
//! Extracts casper/, .disk/ and dists/ — the last one is what the live
//! environment's /cdrom apt source (`file:/cdrom`) needs (`apt-get update`
//! fails without dists/<suite>/Release). pool/ is not extracted on purpose;
//! packages come from the elected network mirror.
//! Usage: cargo run --example extract-casper -- <data_dir> <os_version_id>
//!
//! # Errors
//!
//! Returns a process error when extraction fails.

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(data_dir), Some(os_version_id)) = (args.next(), args.next()) else {
        eprintln!("usage: cargo run --example extract-casper -- <data_dir> <os_version_id>");
        return std::process::ExitCode::FAILURE;
    };
    let base = std::path::PathBuf::from(data_dir)
        .join("os")
        .join(os_version_id);
    let iso = match std::fs::read_dir(&base) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|ext| ext == "iso")),
        Err(_) => None,
    };
    let Some(iso) = iso else {
        eprintln!("no iso found in {}", base.display());
        return std::process::ExitCode::FAILURE;
    };
    let run = || -> std::result::Result<(), String> {
        fn extract_tree(
            iso: &std::path::Path,
            iso_dir: &str,
            base: &std::path::Path,
        ) -> std::result::Result<(), String> {
            let entries = ipxe_loco_rs::iso::list_dir(iso, iso_dir)
                .map_err(|e| format!("listing {iso_dir}/ failed: {e}"))?;
            for (name, _entry, is_dir) in entries {
                let iso_path = format!("{iso_dir}/{name}");
                if is_dir {
                    extract_tree(iso, &iso_path, base)?;
                    continue;
                }
                let dest = base.join(&iso_path);
                std::fs::create_dir_all(dest.parent().expect("dest has a parent"))
                    .map_err(|e| format!("creating {} failed: {e}", dest.display()))?;
                let bytes = ipxe_loco_rs::iso::extract_to(iso, &iso_path, &dest)
                    .map_err(|e| format!("extracting {iso_path} failed: {e}"))?;
                println!("{iso_path} -> {} ({bytes} bytes)", dest.display());
                if iso_path == "casper/vmlinuz" || iso_path == "casper/initrd" {
                    let root_dest = base.join(&name);
                    std::fs::copy(&dest, &root_dest).map_err(|e| {
                        format!("copying {name} to {} failed: {e}", root_dest.display())
                    })?;
                    println!("{name} -> {}", root_dest.display());
                }
            }
            Ok(())
        }
        for iso_dir in ["casper", ".disk", "dists"] {
            extract_tree(&iso, iso_dir, &base)?;
        }
        Ok(())
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

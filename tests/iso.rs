use std::io::Write;

use ipxe_loco_rs::iso;

const SECTOR: usize = 2048;

/// One ISO9660 directory record: extent LBA, byte size, dir flag, name.
/// `rr_name` adds a Rock Ridge NM entry with the original file name.
fn record(lba: u32, size: u32, is_dir: bool, name: &[u8], rr_name: Option<&str>) -> Vec<u8> {
    let mut r = vec![0u8; 33];
    r[2..6].copy_from_slice(&lba.to_le_bytes());
    r[6..10].copy_from_slice(&lba.to_be_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[14..18].copy_from_slice(&size.to_be_bytes());
    r[25] = if is_dir { 0b10 } else { 0 };
    r[32] = name.len() as u8;
    r.extend_from_slice(name);
    // the system use area starts after the name field, padded to an even
    // offset (33 is odd, so even-length names get a pad byte)
    if !(33 + name.len()).is_multiple_of(2) {
        r.push(0);
    }
    if let Some(rr) = rr_name {
        // SUSP NM entry: "NM", length, version 1, flags 0, name fragment
        r.extend_from_slice(b"NM");
        r.push((5 + rr.len()) as u8);
        r.push(1);
        r.push(0);
        r.extend_from_slice(rr.as_bytes());
    }
    // records themselves are padded to an even length
    if !r.len().is_multiple_of(2) {
        r.push(0);
    }
    r[0] = r.len() as u8;
    r
}

fn pad_to_sector(buf: &mut Vec<u8>) {
    let rem = buf.len() % SECTOR;
    if rem != 0 {
        buf.extend(std::iter::repeat_n(0, SECTOR - rem));
    }
}

/// Build a minimal ISO with /CASPER/VMLINUZ.;1 containing FAKE-KERNEL-BYTES
/// plus a Rock Ridge-named casper/filesystem.squashfs entry: PVD at LBA 16,
/// root dir at LBA 17, casper dir at LBA 18, kernel at LBA 19, squashfs at
/// LBA 20.
fn build_minimal_iso(path: &std::path::Path) {
    let kernel = b"FAKE-KERNEL-BYTES";
    let squash = b"FAKE-SQUASHFS";
    let mut root = Vec::new();
    root.extend_from_slice(&record(17, SECTOR as u32, true, &[0], None));
    root.extend_from_slice(&record(17, SECTOR as u32, true, &[1], None));
    root.extend_from_slice(&record(18, SECTOR as u32, true, b"CASPER", None));
    pad_to_sector(&mut root);

    let mut casper = Vec::new();
    casper.extend_from_slice(&record(18, SECTOR as u32, true, &[0], None));
    casper.extend_from_slice(&record(17, SECTOR as u32, true, &[1], None));
    casper.extend_from_slice(&record(19, kernel.len() as u32, false, b"VMLINUZ.;1", None));
    casper.extend_from_slice(&record(
        20,
        squash.len() as u32,
        false,
        b"FILESYST.;1",
        Some("filesystem.squashfs"),
    ));
    pad_to_sector(&mut casper);

    let mut iso = vec![0u8; 16 * SECTOR];
    let mut pvd = vec![0u8; SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    let root_record = record(17, SECTOR as u32, true, &[0], None);
    pvd[156..156 + root_record.len()].copy_from_slice(&root_record);
    iso.extend_from_slice(&pvd);
    iso.extend_from_slice(&root);
    iso.extend_from_slice(&casper);
    iso.extend_from_slice(kernel);
    pad_to_sector(&mut iso); // kernel sits at LBA 19
    iso.extend_from_slice(squash); // squashfs sits at LBA 20
    pad_to_sector(&mut iso);

    let mut file = std::fs::File::create(path).expect("write iso");
    file.write_all(&iso).expect("write iso");
}

#[test]
fn finds_and_extracts_file_from_iso() {
    let mut path = std::env::temp_dir();
    path.push(format!("ipxe-iso-test-{}.img", std::process::id()));
    build_minimal_iso(&path);

    let entry = iso::find(&path, "casper/vmlinuz").expect("entry located");
    assert_eq!(entry.size, 17, "file size from the directory record");

    let mut dest = path.clone();
    dest.set_extension("out");
    let copied = iso::extract_to(&path, "casper/vmlinuz", &dest).expect("extract");
    assert_eq!(copied, 17);
    assert_eq!(
        std::fs::read(&dest).expect("read dest"),
        b"FAKE-KERNEL-BYTES"
    );

    // lookups are name-normalized: the plain ISO9660 name is VMLINUZ.;1
    let entry2 = iso::find(&path, "CASPER/VMLINUZ.;1").expect("normalized lookup");
    assert_eq!(entry2.size, entry.size);

    // Rock Ridge NM names survive: the mangled plain name is FILESYST.;1
    let squash = iso::find(&path, "casper/filesystem.squashfs").expect("rock ridge lookup");
    assert_eq!(squash.size, 13, "squashfs size from the directory record");
    let mut dest2 = path.clone();
    dest2.set_extension("sq");
    let copied2 = iso::extract_to(&path, "casper/filesystem.squashfs", &dest2).expect("extract");
    assert_eq!(copied2, 13);
    assert_eq!(std::fs::read(&dest2).expect("read dest"), b"FAKE-SQUASHFS");

    let missing = iso::find(&path, "casper/initrd").expect_err("missing file");
    assert!(matches!(missing, iso::IsoError::NotFound(_)));

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::remove_file(&dest2);
}

#[test]
fn rejects_non_iso_files() {
    let mut path = std::env::temp_dir();
    path.push(format!("ipxe-iso-notiso-{}.img", std::process::id()));
    std::fs::write(&path, b"definitely not an iso").expect("write junk");

    let err = iso::find(&path, "casper/vmlinuz").expect_err("not an iso");
    assert!(matches!(err, iso::IsoError::NotAnIso));

    let _ = std::fs::remove_file(&path);
}

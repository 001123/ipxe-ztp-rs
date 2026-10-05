//! Minimal read-only ISO9660 walker.
//!
//! Just enough to pull files out of a downloaded Ubuntu ISO: the casper
//! kernel and initrd only exist inside the ISO, so the downloader extracts
//! them locally instead of fetching them from a mirror (no mirror hosts
//! them). ISO9660 directory records never span sector boundaries, so a
//! per-sector scan is sufficient. Rock Ridge NM names are used verbatim —
//! the original case matters (`dists/<suite>/Release` for apt) — while
//! records without Rock Ridge fall back to the plain name, case-folded
//! with the `;version` suffix stripped, which maps `VMLINUZ.;1` to
//! `vmlinuz`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SECTOR: u64 = 2048;

/// Location and length of a file inside the ISO, in bytes.
#[derive(Debug, Clone, Copy)]
pub struct IsoEntry {
    pub lba: u64,
    pub size: u64,
}

/// Anything that prevents locating the requested file: not an ISO, a
/// truncated image, or the path simply not present.
#[derive(Debug)]
pub enum IsoError {
    Io(std::io::Error),
    NotAnIso,
    NotFound(String),
    Truncated(String),
}

impl std::fmt::Display for IsoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsoError::Io(e) => write!(f, "io error reading iso: {e}"),
            IsoError::NotAnIso => write!(f, "not an ISO9660 image (no CD001 descriptor)"),
            IsoError::NotFound(path) => write!(f, "{path} not found in iso"),
            IsoError::Truncated(msg) => write!(f, "iso truncated: {msg}"),
        }
    }
}

impl From<std::io::Error> for IsoError {
    fn from(e: std::io::Error) -> Self {
        IsoError::Io(e)
    }
}

/// Locate `path` (e.g. `casper/vmlinuz`) inside the ISO image.
///
/// # Errors
///
/// When the file cannot be read or located.
pub fn find(iso: &Path, path: &str) -> Result<IsoEntry, IsoError> {
    let mut file = File::open(iso)?;
    find_in(&mut file, path, true)
}

/// Locate a directory inside the ISO image (for [`list_dir`]).
///
/// # Errors
///
/// When the directory cannot be read or located.
fn find_dir_in(file: &mut File, path: &str) -> Result<IsoEntry, IsoError> {
    find_in(file, path, false)
}

/// [`find`] against an already-open image (avoids re-opening per lookup);
/// `expect_file` decides whether the final segment must be a file or a
/// directory.
fn find_in(file: &mut File, path: &str, expect_file: bool) -> Result<IsoEntry, IsoError> {
    // The primary volume descriptor lives at sector 16 and starts with the
    // magic "CD001"; its root directory record sits at offset 156.
    let mut pvd = [0u8; SECTOR as usize];
    file.seek(SeekFrom::Start(16 * SECTOR))?;
    // an image too small to hold sector 16 is not an ISO at all
    file.read_exact(&mut pvd).map_err(|_| IsoError::NotAnIso)?;
    if &pvd[1..6] != b"CD001" || pvd[0] != 1 {
        return Err(IsoError::NotAnIso);
    }
    let root = DirRecord::parse(&pvd[156..190]).ok_or(IsoError::NotAnIso)?;

    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(normalize_name)
        .collect();
    let mut current = root;
    for (i, segment) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        let record = walk_dir(file, current.entry(), segment, is_last && expect_file)?;
        current = record;
    }
    Ok(current.entry())
}

/// Extract `path` from the ISO into `dest`, streaming (an initrd can be
/// hundreds of MiB). Returns the number of bytes written.
///
/// # Errors
///
/// When the file cannot be read, located, or written.
pub fn extract_to(iso: &Path, path: &str, dest: &Path) -> Result<u64, IsoError> {
    let entry = find(iso, path)?;
    let mut file = File::open(iso)?;
    file.seek(SeekFrom::Start(entry.lba * SECTOR))?;
    let mut out = File::create(dest)?;
    let mut limited = file.by_ref().take(entry.size);
    let copied = std::io::copy(&mut limited, &mut out)?;
    if copied < entry.size {
        return Err(IsoError::Truncated(format!(
            "{path}: expected {size} bytes, image ended after {copied}",
            size = entry.size
        )));
    }
    Ok(copied)
}

#[derive(Debug, Clone, Copy)]
struct DirRecord {
    lba: u64,
    size: u64,
    is_dir: bool,
}

impl DirRecord {
    fn parse(raw: &[u8]) -> Option<Self> {
        Some(Self {
            lba: u32::from_le_bytes(raw[2..6].try_into().ok()?) as u64,
            size: u32::from_le_bytes(raw[10..14].try_into().ok()?) as u64,
            is_dir: raw[25] & 0b10 != 0,
        })
    }

    fn entry(&self) -> IsoEntry {
        IsoEntry {
            lba: self.lba,
            size: self.size,
        }
    }
}

/// List the entries of a directory inside the ISO (e.g. `casper`): each is
/// the normalized name plus its location and whether it is a directory.
/// Self/parent pointers are skipped.
///
/// # Errors
///
/// When the directory cannot be read or located.
pub fn list_dir(iso: &Path, path: &str) -> Result<Vec<(String, IsoEntry, bool)>, IsoError> {
    let mut file = File::open(iso)?;
    let entry = find_dir_in(&mut file, path)?;
    let mut buf = vec![0u8; entry.size as usize];
    file.seek(SeekFrom::Start(entry.lba * SECTOR))?;
    file.read_exact(&mut buf)
        .map_err(|_| IsoError::Truncated(format!("directory extent of {path} out of bounds")))?;

    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        let len = buf[pos];
        if len == 0 {
            // records never span sectors: zero padding means the rest of
            // this sector is empty, continue at the next one
            pos = pos.next_multiple_of(SECTOR as usize);
            if pos >= buf.len() {
                break;
            }
            continue;
        }
        let raw = &buf[pos..pos + len as usize];
        pos += len as usize;
        if raw.len() < 34 {
            continue;
        }
        let name_len = raw[32] as usize;
        let plain_name = String::from_utf8_lossy(&raw[33..33 + name_len]);
        if matches!(plain_name.as_bytes(), [0] | [1]) {
            continue; // self / parent pointers
        }
        let record = DirRecord::parse(raw)
            .ok_or_else(|| IsoError::Truncated(format!("malformed directory record in {path}")))?;
        let display = rock_ridge_name(raw).unwrap_or_else(|| normalize_name(&plain_name));
        out.push((display, record.entry(), record.is_dir));
    }
    Ok(out)
}

/// Scan a directory extent for `wanted` (already normalized). `expect_file`
/// decides the kind the final match must have; intermediate walks always
/// expect directories.
fn walk_dir(
    file: &mut File,
    dir: IsoEntry,
    wanted: &str,
    expect_file: bool,
) -> Result<DirRecord, IsoError> {
    if dir.lba == 0 || dir.size == 0 {
        return Err(IsoError::Truncated(format!(
            "empty directory record for {wanted}"
        )));
    }

    let mut buf = vec![0u8; dir.size as usize];
    file.seek(SeekFrom::Start(dir.lba * SECTOR))?;
    file.read_exact(&mut buf)
        .map_err(|_| IsoError::Truncated(format!("directory extent of {wanted} out of bounds")))?;

    let mut pos = 0usize;
    while pos < buf.len() {
        let len = buf[pos];
        if len == 0 {
            // records never span sectors: zero padding means the rest of
            // this sector is empty, continue at the next one
            pos = pos.next_multiple_of(SECTOR as usize);
            if pos >= buf.len() {
                break;
            }
            continue;
        }
        let raw = &buf[pos..pos + len as usize];
        pos += len as usize;
        if raw.len() < 34 {
            continue;
        }
        let name_len = raw[32] as usize;
        let plain_name = String::from_utf8_lossy(&raw[33..33 + name_len]);
        if matches!(plain_name.as_bytes(), [0] | [1]) {
            continue; // self / parent pointers
        }
        let record = DirRecord::parse(raw).ok_or_else(|| {
            IsoError::Truncated(format!("malformed directory record near {wanted}"))
        })?;
        // Rock Ridge NM preserves the real name ("filesystem.squashfs"); the
        // plain ISO9660 name is a lossy fallback ("FILESY..;1"-style)
        let name = rock_ridge_name(raw).unwrap_or_else(|| plain_name.to_string());
        let normalized = normalize_name(&name);
        if normalized == wanted && record.is_dir == expect_file {
            // the kind is wrong: walking a file found a directory (or vice
            // versa), which is not the entry we are after
            let kind = if expect_file { "file" } else { "directory" };
            return Err(IsoError::NotFound(format!("{wanted} ({kind} expected)")));
        }
        if normalized == wanted {
            return Ok(record);
        }
    }
    Err(IsoError::NotFound(wanted.to_string()))
}

/// Extract the Rock Ridge `NM` entry from a directory record's system use
/// area: the original filename that ISO9660's 8.3-style names mangle.
fn rock_ridge_name(raw: &[u8]) -> Option<String> {
    let name_len = raw[32] as usize;
    // the system use area starts right after the name field; when that
    // offset is odd a zero pad byte sits in between
    let mut pos = 33 + name_len;
    if pos % 2 == 1 {
        pos += 1;
    }
    let end = raw[0] as usize;
    let mut name: Vec<u8> = Vec::new();
    while pos + 4 <= end {
        let len = raw[pos + 2] as usize;
        if len < 4 || pos + len > end {
            break; // not SUSP-shaped or malformed: bail into the fallback
        }
        if &raw[pos..pos + 2] == b"NM" && raw[pos + 3] == 1 {
            // payload: flags byte, then the name fragment. The CONTINUE
            // flag (name split across records) is not handled here —
            // casper's file names are far below the 250-char single-entry limit.
            name.extend_from_slice(&raw[pos + 5..pos + len]);
        }
        pos += len;
    }
    if name.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&name).to_string())
    }
}

/// `VMLINUZ.;1` → `vmlinuz`: lowercase, strip the `;version` suffix, and a
/// trailing dot from mangled plain names. Used for case-insensitive path
/// matching and as the extracted name for records without Rock Ridge.
/// Records WITH a Rock Ridge NM name keep that name verbatim instead —
/// the original case matters (apt requires `dists/<suite>/Release`).
fn normalize_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    let no_version = lowered.split(';').next().unwrap_or(&lowered);
    no_version
        .strip_suffix('.')
        .unwrap_or(no_version)
        .to_string()
}

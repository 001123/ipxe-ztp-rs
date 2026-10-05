//! Parsing and validation for a list of apt package names.
//!
//! Package lists are stored as plain text — one package per line — in the
//! machine's `packages` column and in the `packages` setting. Everywhere they
//! are accepted, they are normalized through [`normalize_list`] and each
//! entry checked with [`is_valid_name`] so nothing but well-formed Debian
//! package names reaches the autoinstall seed.

use std::sync::LazyLock;

use regex::Regex;

/// A Debian package name: letters/digits, with `+`, `.` and `-` allowed after
/// the first character. An optional `=version` pin (`curl=7.81.0-1`) and an
/// optional architecture suffix (`libc6:amd64`) are also accepted.
static PACKAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9+._-]*(:[a-zA-Z0-9-]+)?(=[a-zA-Z0-9][a-zA-Z0-9+._:~-]*)?$")
        .expect("package name regex must be valid")
});

/// Upper bound on list size — a sane guard against pasting an entire
/// `dpkg -l` into a machine row.
pub const MAX_PACKAGES: usize = 100;

/// Splits a stored package list on newlines, commas and whitespace, trims
/// entries and drops empties and duplicates while preserving first-seen
/// order.
#[must_use]
pub fn normalize_list(text: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for entry in text.split(['\n', '\r', ',', ' ', '\t']) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if !seen.contains(&entry.to_string()) {
            seen.push(entry.to_string());
        }
    }
    seen
}

/// Whether a single entry is a well-formed Debian package name (optionally
/// pinned with `=version` or architecture-qualified with `:arch`).
#[must_use]
pub fn is_valid_name(entry: &str) -> bool {
    PACKAGE_RE.is_match(entry)
}

/// Validates a stored package list: size cap plus per-entry name check.
/// Returns the offending entry (if any) so callers can point at it.
pub fn validate_list(text: &str) -> Result<(), String> {
    let packages = normalize_list(text);
    if packages.len() > MAX_PACKAGES {
        return Err(format!("must have at most {} packages.", MAX_PACKAGES));
    }
    for package in &packages {
        if !is_valid_name(package) {
            return Err(format!("invalid package name: {package}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_list_splits_on_newlines_commas_and_spaces() {
        assert_eq!(
            normalize_list("curl\nhtop, vim jq"),
            vec!["curl", "htop", "vim", "jq"]
        );
    }

    #[test]
    fn normalize_list_drops_empty_and_duplicates_preserving_order() {
        assert_eq!(
            normalize_list("curl\n\ncurl, htop\n  \nhtop"),
            vec!["curl", "htop"]
        );
    }

    #[test]
    fn normalize_list_empty_input() {
        assert!(normalize_list("").is_empty());
        assert!(normalize_list("  \n  \n").is_empty());
    }

    #[test]
    fn is_valid_name_accepts_plain_pinned_and_qualified() {
        assert!(is_valid_name("curl"));
        assert!(is_valid_name("htop"));
        assert!(is_valid_name("libc6:amd64"));
        assert!(is_valid_name("curl=7.81.0-1"));
        assert!(is_valid_name("openjdk-21-jre-headless"));
    }

    #[test]
    fn is_valid_name_rejects_garbage() {
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("curl; rm -rf /"));
        assert!(!is_valid_name("$(uname -r)"));
        assert!(!is_valid_name("my package"));
        assert!(!is_valid_name("+leading"));
        assert!(!is_valid_name("curl "));
    }

    #[test]
    fn validate_list_points_at_offender() {
        assert!(validate_list("curl\nhtop").is_ok());
        // `;` is not a delimiter, so this stays one invalid entry
        let err = validate_list("curl\npkg;;x").unwrap_err();
        assert!(err.contains("pkg;;x"));
        let too_many = (0..MAX_PACKAGES + 1)
            .map(|i| format!("pkg{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(validate_list(&too_many).is_err());
    }
}

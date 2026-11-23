//! Updates of the tool itself: which GitHub release is newer and which of its files fits this machine.

/// A published version of the tool, with the binary for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Without the tag's `v` (e.g. `0.2.0`).
    pub version: String,
    pub binary_url: String,
    /// `<sha256>  <file name>`, as written by `sha256sum`.
    pub checksum_url: String,
}

/// The release file built for this platform (see `.github/workflows/release.yml`), if any.
pub const BINARY: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("scout-rs-x86_64-linux")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("scout-rs-x86_64-windows.exe")
} else {
    None
};

/// `v0.10.2` → [0, 10, 2]; None when it is not a version.
fn parse(version: &str) -> Option<Vec<u64>> {
    let version = version.strip_prefix('v').unwrap_or(version);
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// Whether `latest` (a tag such as `v0.2.0`) is newer than `current` (such as `0.1.0`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// The hash from a `sha256sum` line, in lowercase.
pub fn parse_checksum(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?;
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.3"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }

    #[test]
    fn reads_sha256sum_lines() {
        let hash = "AB".repeat(32);
        assert_eq!(
            parse_checksum(&format!("{hash}  scout-rs-x86_64-linux\n")),
            Some("ab".repeat(32))
        );
        assert_eq!(parse_checksum("abc  file"), None);
        assert_eq!(parse_checksum(""), None);
    }
}

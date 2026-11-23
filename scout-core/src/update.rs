//! Updates of the tool and of the Scout addon: which GitHub release is newer, which file to take.

/// A published version, with the file to download (tool binary for this platform, or addon zip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Without the tag's `v` (e.g. `0.2.0`).
    pub version: String,
    pub url: String,
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

/// The addon's release file: a `Scout/` folder to unpack in `Interface/AddOns`.
pub const ADDON_ARCHIVE: &str = "Scout.zip";

/// The Scout addon in the WoW folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddonInstall {
    Missing,
    /// A link to a working copy (or a git clone): never replaced by the tool.
    Dev(Option<String>),
    /// Installed from a release (or by hand), with the `## Version` of its `Scout.toc`.
    Installed(Option<String>),
}

impl AddonInstall {
    /// The release to offer: when the addon is missing, or older than `latest` (or without a version).
    pub fn offer<'a>(&self, latest: Option<&'a Release>) -> Option<&'a Release> {
        let latest = latest?;
        match self {
            Self::Missing | Self::Installed(None) => Some(latest),
            Self::Installed(Some(version)) => is_newer(&latest.version, version).then_some(latest),
            Self::Dev(_) => None,
        }
    }
}

/// `## Version: 0.1.0` of a `.toc` file.
pub fn toc_version(toc: &str) -> Option<String> {
    toc.lines()
        .find_map(|line| line.strip_prefix("## Version:"))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

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
    fn offers_the_addon_when_missing_or_older() {
        let release = Release {
            version: "0.2.0".into(),
            url: String::new(),
            checksum_url: String::new(),
        };
        let latest = Some(&release);
        let installed = |v: &str| AddonInstall::Installed(Some(v.into()));
        assert!(AddonInstall::Missing.offer(latest).is_some());
        assert!(AddonInstall::Installed(None).offer(latest).is_some());
        assert!(installed("0.1.0").offer(latest).is_some());
        assert!(installed("0.2.0").offer(latest).is_none());
        assert!(
            AddonInstall::Dev(Some("0.1.0".into()))
                .offer(latest)
                .is_none()
        );
        assert!(AddonInstall::Missing.offer(None).is_none());
    }

    #[test]
    fn reads_the_toc_version() {
        let toc = "## Interface: 16001\r\n## Title: Scout\r\n## Version: 0.1.0\r\n";
        assert_eq!(toc_version(toc).as_deref(), Some("0.1.0"));
        assert_eq!(toc_version("## Title: Scout"), None);
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

//! The reference package the server serves (loot, merchants and item names, a gzipped SQLite file), and its version.

use serde::{Deserialize, Serialize};

/// Schema of the tables in the package (`drop_*`, `vendor_prices`, `items`): raise it when their columns change.
pub const REFERENCE_FORMAT: i64 = 2;

/// Describes the package served by the server (`reference.json` next to it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceManifest {
    pub format: i64,
    /// Unix seconds, when the package was exported.
    pub updated: i64,
    /// Size of the gzipped package.
    pub bytes: u64,
}

impl ReferenceManifest {
    /// Whether this package should replace the local one, given as (format, updated):
    /// none yet, another format or an older date.
    pub fn newer_than(&self, local: Option<(i64, i64)>) -> bool {
        match local {
            None => true,
            Some((format, updated)) => format != self.format || updated < self.updated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: ReferenceManifest = ReferenceManifest {
        format: 2,
        updated: 100,
        bytes: 5,
    };

    #[test]
    fn newer_than_compares_format_then_date() {
        assert!(M.newer_than(None));
        assert!(M.newer_than(Some((2, 99))));
        assert!(!M.newer_than(Some((2, 100))));
        assert!(!M.newer_than(Some((2, 101))));
        assert!(M.newer_than(Some((3, 500))));
        assert!(M.newer_than(Some((1, 500))));
    }
}

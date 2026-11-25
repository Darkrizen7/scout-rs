//! Raw snapshots on disk: `raw/YYYY/MM/DD/{house}_{kind}_{HHMMSS}.json.zst` (UTC) under the data folder.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::blizzard::Kind;

pub struct Storage {
    data_dir: PathBuf,
    level: i32,
}

impl Storage {
    pub fn new(data_dir: &Path, level: i32) -> Self {
        Self {
            data_dir: data_dir.to_owned(),
            level,
        }
    }

    /// Compresses `bytes` to `relative` (atomically: a crash never leaves half a file); returns its size.
    pub fn write(&self, relative: &str, bytes: &[u8]) -> Result<u64> {
        let path = self.data_dir.join(relative);
        let dir = path.parent().context("chemin sans dossier")?;
        fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
        let compressed = zstd::bulk::compress(bytes, self.level).context("compression zstd")?;
        let tmp = path.with_extension("tmp");
        let mut file =
            fs::File::create(&tmp).with_context(|| format!("écriture de {}", tmp.display()))?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        fs::rename(&tmp, &path)?;
        Ok(compressed.len() as u64)
    }

    /// Bytes still available to us on the data disk.
    pub fn free_bytes(&self) -> Result<u64> {
        let stat = rustix::fs::statvfs(&self.data_dir).context("place libre sur le disque")?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }
}

/// Path of a raw snapshot, relative to the data folder.
pub fn relative_path(house: u32, kind: Kind, unix: i64) -> String {
    let (days, secs) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    let (h, min, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    format!(
        "raw/{y:04}/{m:02}/{d:02}/{house}_{}_{h:02}{min:02}{s:02}.json.zst",
        kind.as_str()
    )
}

/// Days since 1970-01-01 to a (year, month, day) of the proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_paths_in_utc() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        // 2026-11-25 09:15:07 UTC
        assert_eq!(
            relative_path(114, Kind::Commodities, 1_795_598_107),
            "raw/2026/11/25/114_commodities_091507.json.zst"
        );
    }

    #[test]
    fn writes_a_readable_compressed_file() {
        let dir = std::env::temp_dir().join(format!("scout-server-storage-{}", std::process::id()));
        let storage = Storage::new(&dir, 3);
        let body = br#"{"auctions":[]}"#.repeat(100);
        let size = storage.write("raw/2026/11/25/a.json.zst", &body).unwrap();
        let read = fs::read(dir.join("raw/2026/11/25/a.json.zst")).unwrap();
        assert_eq!(read.len() as u64, size);
        assert_eq!(zstd::decode_all(&read[..]).unwrap(), body);
        assert!(storage.free_bytes().unwrap() > 0);
        fs::remove_dir_all(dir).unwrap();
    }
}

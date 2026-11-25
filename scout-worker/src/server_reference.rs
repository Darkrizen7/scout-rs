//! The reference package of the server (`scout-server/data/`): loot and merchants of a filled database,
//! gzipped, with its manifest (`scout_core::reference::ReferenceManifest`). GPL-3.0-or-later like the loot it holds.
//! Made by `cargo run -p scout-worker --example server_reference`, committed, served after a server deploy.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use flate2::Compression;
use flate2::write::GzEncoder;
use scout_core::reference::{REFERENCE_FORMAT, ReferenceManifest};
use scout_db::Db;

/// Writes `reference.db.gz` and `reference.json` in `dir` from the database at `source` (only read).
pub async fn export_server_reference(source: &Path, dir: &Path) -> Result<ReferenceManifest> {
    fs::create_dir_all(dir)?;
    let unpacked = dir.join("reference.db.tmp");
    let _ = fs::remove_file(&unpacked);
    let result = async {
        Db::export_loot_vendors(source, &unpacked).await?;
        let package = dir.join("reference.db.gz");
        let mut out = GzEncoder::new(BufWriter::new(File::create(&package)?), Compression::best());
        io::copy(&mut BufReader::new(File::open(&unpacked)?), &mut out)?;
        out.finish()?.flush()?;
        let manifest = ReferenceManifest {
            format: REFERENCE_FORMAT,
            updated: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64,
            bytes: fs::metadata(&package)?.len(),
        };
        fs::write(
            dir.join("reference.json"),
            serde_json::to_string_pretty(&manifest)? + "\n",
        )?;
        anyhow::Ok(manifest)
    }
    .await;
    let _ = fs::remove_file(&unpacked);
    result
}

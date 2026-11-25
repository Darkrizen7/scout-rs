//! The reference database shipped inside the executable (`data/reference.db.gz`, GPL-3.0-or-later like
//! the loot it holds; made from a filled database by `cargo run -p scout-worker --example reference`).
//! Copied into the local database at the first start after each new one, where newer than the local data.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use scout_db::{Db, ReferenceUpdate};

const EMBEDDED: &[u8] = include_bytes!("../data/reference.db.gz");

/// Identifies a gzip file: its trailer (CRC-32 and size of the uncompressed data).
fn id(gzip: &[u8]) -> String {
    gzip[gzip.len().saturating_sub(8)..]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn unpack(gzip: &[u8], dest: &Path) -> Result<()> {
    let mut out = BufWriter::new(File::create(dest)?);
    io::copy(&mut GzDecoder::new(gzip), &mut out)?;
    out.flush()?;
    Ok(())
}

/// Applies the embedded reference database unless it already was, unpacked meanwhile at `unpacked`.
/// None when there was nothing to do.
pub(crate) async fn apply(db: &Db, unpacked: &Path) -> Result<Option<ReferenceUpdate>> {
    let id = id(EMBEDDED);
    if db.reference_id().await?.as_deref() == Some(id.as_str()) {
        return Ok(None);
    }
    unpack(EMBEDDED, unpacked).context("décompression")?;
    let update = db.apply_reference(unpacked).await;
    let _ = fs::remove_file(unpacked);
    let update = update?;
    db.set_reference_id(&id).await?;
    Ok(Some(update))
}

/// Writes a new reference database, compressed, at `dest` from the database at `source`.
pub async fn export(source: &Path, dest: &Path) -> Result<()> {
    let unpacked = dest.with_extension("tmp");
    let _ = fs::remove_file(&unpacked);
    let result = async {
        Db::export_reference(source, &unpacked).await?;
        let mut out = GzEncoder::new(BufWriter::new(File::create(dest)?), Compression::best());
        io::copy(&mut BufReader::new(File::open(&unpacked)?), &mut out)?;
        out.finish()?.flush()?;
        anyhow::Ok(())
    }
    .await;
    let _ = fs::remove_file(&unpacked);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use scout_core::GAME_FORMAT;

    /// Version of the reference loot. Frozen since Forever Guide is no longer imported: the fingerprint of
    /// the embedded loot starts with it.
    const DROPS_FORMAT: u32 = 2;

    /// The embedded database must match this version of the tool: regenerate it (see the module doc) when
    /// `GAME_FORMAT` changes.
    #[tokio::test]
    async fn embedded_reference_is_up_to_date() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("scout-rs-embedded-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let db = Db::open(&dir.join("test.db")).await?;
        let unpacked = dir.join("reference.tmp");
        let update = apply(&db, &unpacked).await?;
        assert_eq!(
            update,
            Some(ReferenceUpdate {
                game: true,
                drops: true
            })
        );
        let (_, format, _) = db.game_version().await?.context("pas de données du jeu")?;
        assert_eq!(format, GAME_FORMAT);
        assert!(db.load_game().await?.complete, "données du jeu incomplètes");
        let fingerprint = db.drops_fingerprint().await?.context("pas de butin")?;
        assert!(
            fingerprint.starts_with(&format!("v{DROPS_FORMAT};")),
            "{fingerprint}"
        );
        // Applied once only.
        assert_eq!(apply(&db, &unpacked).await?, None);
        db.close().await;
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}

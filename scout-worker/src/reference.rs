//! Downloads the loot and merchants package of the price server (GPL-3.0-or-later like the loot it holds;
//! made by `cargo run -p scout-worker --example server_reference`) and applies it to the local database.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use scout_core::reference::REFERENCE_FORMAT;
use scout_db::Db;
use scout_sources::price_server;

use crate::message::Event;

fn unpack(gzip: &[u8], dest: &Path) -> Result<()> {
    let mut out = BufWriter::new(File::create(dest)?);
    io::copy(&mut GzDecoder::new(gzip), &mut out)?;
    out.flush()?;
    Ok(())
}

/// What `sync_server` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerSync {
    /// The server has no package for this format.
    Unavailable,
    UpToDate,
    Installed,
}

/// Downloads the loot and merchants package of `server` when it is newer than the one applied, and applies
/// it (unpacked meanwhile at `unpacked`, removed afterwards). Blocks on the network.
pub async fn sync_server(
    db: &Db,
    server: &str,
    unpacked: &Path,
    send: &dyn Fn(Event),
) -> Result<ServerSync> {
    let Some(manifest) = price_server::reference_latest(server, REFERENCE_FORMAT)? else {
        return Ok(ServerSync::Unavailable);
    };
    if !manifest.newer_than(db.server_reference().await?) {
        return Ok(ServerSync::UpToDate);
    }
    send(Event::Status(format!(
        "Téléchargement du butin et des marchands de référence ({} Ko)…",
        manifest.bytes / 1000
    )));
    let gzip = price_server::reference_package(server, &manifest)?;
    let result = async {
        unpack(&gzip, unpacked).context("décompression du butin de référence")?;
        db.apply_loot_vendors(unpacked).await
    }
    .await;
    let _ = fs::remove_file(unpacked);
    result?;
    db.set_server_reference(manifest.format, manifest.updated)
        .await?;
    Ok(ServerSync::Installed)
}

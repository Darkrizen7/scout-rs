//! Makes the reference package served by scout-server (loot and merchants, `scout-server/data/`) from a
//! database: `cargo run -p scout-worker --example server_reference [path of the database]`
//! (default: the local one, only read). Then commit the two files and deploy the server.

use std::path::{Path, PathBuf};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let source = match std::env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => scout_worker::database_path()?,
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scout-server/data");
    let manifest = scout_worker::export_server_reference(&source, &dir).await?;
    println!(
        "{} : format {}, {:.2} Mo, depuis {}",
        dir.display(),
        manifest.format,
        manifest.bytes as f64 / 1e6,
        source.display()
    );
    Ok(())
}

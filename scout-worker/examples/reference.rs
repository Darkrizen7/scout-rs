//! Makes the reference database shipped with the tool (`scout-worker/data/reference.db.gz`) from the local
//! database: `cargo run -p scout-worker --example reference`. Needs the game data (Réglages → Mettre à jour
//! les données du jeu) and Forever Guide's loot (an import with Forever Guide installed).

use std::path::Path;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let source = scout_worker::database_path()?;
    let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/reference.db.gz");
    scout_worker::export_reference(&source, &dest).await?;
    let size = std::fs::metadata(&dest)?.len();
    println!(
        "{} : {:.1} Mo, depuis {}",
        dest.display(),
        size as f64 / 1e6,
        source.display()
    );
    Ok(())
}

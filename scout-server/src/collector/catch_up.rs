//! Start-up catch-up of the sales of snapshots written before `sales` existed (or interrupted).

use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

use anyhow::{Context, Result};

use super::Collector;
use crate::sales;

impl Collector {
    /// Compares each pending snapshot with the one before it, one at a time under the heavy permit.
    pub(super) async fn sales_pending(&self) -> Result<()> {
        let pending = self.db.pending_sales().await?;
        if pending.is_empty() {
            return Ok(());
        }
        let start = Instant::now();
        for p in &pending {
            let _permit = self.heavy.acquire().await?;
            let previous = self
                .db
                .previous_snapshot(p.house, &p.kind, p.fetched_at)
                .await?;
            let previous = previous.and_then(|(at, path)| Some((at, self.storage.path(&path?))));
            let (path, ts) = (self.storage.path(&p.path), p.fetched_at);
            let found = tokio::task::spawn_blocking(move || -> Result<_> {
                let file =
                    File::open(&path).with_context(|| format!("lecture de {}", path.display()))?;
                let newer = zstd::stream::read::Decoder::new(BufReader::new(file))
                    .context("décompression")?;
                sales::since_previous(previous, ts, newer)
            })
            .await??;
            self.db.add_sales(p, &found).await?;
        }
        eprintln!(
            "ventes calculées pour {} anciens instantanés en {:.1} s",
            pending.len(),
            start.elapsed().as_secs_f32()
        );
        Ok(())
    }
}

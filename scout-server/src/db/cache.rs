//! In-memory cache of `Db::latest`, one entry per house (each region has its own `Db`).
//!
//! Entries are created on demand only, for houses actually requested. An entry holds one
//! `PriceRow` (64 bytes) per item on sale: a few tens of thousands of items, so a few MB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::PriceRow;

#[derive(Default)]
struct Inner {
    rows: HashMap<u32, Arc<[PriceRow]>>,
    /// Bumped by each invalidation: a read that started before one must not be stored.
    generation: u64,
}

#[derive(Clone, Default)]
pub(super) struct LatestCache(Arc<Mutex<Inner>>);

impl LatestCache {
    /// The cached rows of `house`, or the generation to give back to `put`.
    pub(super) fn get(&self, house: u32) -> Result<Arc<[PriceRow]>, u64> {
        let inner = self.0.lock().unwrap();
        inner.rows.get(&house).cloned().ok_or(inner.generation)
    }

    /// Stores rows read at `generation`, unless a write happened since. Returns what to serve.
    pub(super) fn put(&self, house: u32, generation: u64, rows: Vec<PriceRow>) -> Arc<[PriceRow]> {
        let rows: Arc<[PriceRow]> = rows.into();
        let mut inner = self.0.lock().unwrap();
        if inner.generation == generation {
            inner.rows.insert(house, rows.clone());
        }
        rows
    }

    pub(super) fn invalidate(&self, house: u32) {
        let mut inner = self.0.lock().unwrap();
        inner.generation += 1;
        inner.rows.remove(&house);
    }
}

//! Data loaded on first use: asked from the worker once, then kept until the next import.

use std::collections::HashMap;
use std::sync::Arc;

use scout_core::drops::{ItemDrops, Zone, ZoneLoot};
use scout_core::map::Mask;
use scout_worker::{Command, WorkerHandle};

use super::State;

impl State {
    /// The sources of an item once loaded; asks the worker for them (once) otherwise.
    pub fn item_drops(&mut self, item_id: i64, worker: &WorkerHandle) -> Option<&ItemDrops> {
        let loaded = self.drops.as_ref().is_some_and(|(id, _)| *id == item_id);
        if !loaded && self.drops_asked != Some(item_id) {
            self.drops_asked = Some(item_id);
            worker.send(Command::LoadDrops(item_id));
        }
        self.drops
            .as_ref()
            .filter(|(id, _)| *id == item_id)
            .map(|(_, d)| d)
    }

    /// Zones with known loot once loaded; asks the worker for them (once) otherwise.
    pub fn zones(&mut self, worker: &WorkerHandle) -> Option<Arc<Vec<Zone>>> {
        if self.zones.is_none() && !self.zones_asked {
            self.zones_asked = true;
            worker.send(Command::LoadZones);
        }
        self.zones.clone()
    }

    /// The monsters of a zone once loaded; asks the worker for them (once) otherwise.
    pub fn zone_loot(&mut self, zone: &str, worker: &WorkerHandle) -> Option<Arc<ZoneLoot>> {
        if !self.zone_loot.contains_key(zone) && self.zone_asked.insert(zone.to_owned()) {
            worker.send(Command::LoadZone(zone.to_owned()));
        }
        self.zone_loot.get(zone).cloned()
    }

    /// Zone shapes once loaded (empty without OpenMap); asks the worker for them (once) otherwise.
    pub fn map_masks(&mut self, worker: &WorkerHandle) -> Option<Arc<HashMap<i64, Mask>>> {
        if self.masks.is_none() && !self.masks_asked {
            self.masks_asked = true;
            worker.send(Command::LoadMapMasks);
        }
        self.masks.clone()
    }
}

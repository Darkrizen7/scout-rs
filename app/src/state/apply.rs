//! What the worker's events change in the state.

use std::collections::HashSet;
use std::sync::Arc;

use scout_worker::Event;

use super::State;

impl State {
    /// Takes in an event of the worker.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Config(config) => {
                if self.config.as_ref().map(|c| c.hide_obsolete) != Some(config.hide_obsolete) {
                    self.revision += 1;
                }
                self.config = Some(config);
            }
            Event::Busy(task, true) => {
                self.running.insert(task);
            }
            Event::Busy(task, false) => {
                self.running.remove(&task);
            }
            Event::Status(status) => {
                self.status = status;
                self.error = None;
            }
            Event::Error(error) => self.error = Some(error),
            Event::Items(items) => {
                self.index = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item.item_id, i))
                    .collect();
                self.items = items;
                self.revision += 1;
                // Sent after each import: the drops may have changed, the open sheet asks again.
                self.drops = None;
                self.drops_asked = None;
                self.zones = None;
                self.zones_asked = false;
                self.zone_loot.clear();
                self.zone_asked.clear();
            }
            Event::History(id, history) => self.history = Some((id, history)),
            Event::Drops(id, drops) => {
                self.drops = Some((id, *drops));
                self.drops_asked = None;
            }
            Event::Zones(zones) => self.zones = Some(Arc::new(zones)),
            Event::ZoneLoot(loot) => {
                self.zone_asked.remove(&loot.zone);
                self.zone_loot.insert(loot.zone.clone(), Arc::from(loot));
            }
            Event::MapMasks(masks) => self.masks = Some(masks),
            #[cfg(feature = "analysis")]
            Event::Analysis(analysis) => self.analysis = Some(*analysis),
            #[cfg(not(feature = "analysis"))]
            Event::Analysis(_) => {}
            Event::GameData(game) => self.game = game,
            Event::Vendor(vendor) => self.vendor = vendor,
            Event::LatestBuild(build) => self.latest_build = Some(build),
            Event::ScoutAddon(install) => self.scout_addon = Some(install),
            Event::AddonRelease(release) => self.addon_release = Some(release),
            Event::UpdateAvailable(release) => self.update = Some(release),
            Event::UpdateInstalled(version) => self.update_installed = Some(version),
            Event::Quit => self.quit = true,
            Event::ServerHouses(houses) => self.server_houses = Some(houses),
            Event::ZoneFavorites(zones) => self.zone_favorites = zones.into_iter().collect(),
            Event::Favorites(favorites) => {
                let favorites: HashSet<i64> = favorites.into_iter().collect();
                if favorites != self.favorites {
                    self.favorites = favorites;
                    self.revision += 1;
                }
            }
        }
    }
}

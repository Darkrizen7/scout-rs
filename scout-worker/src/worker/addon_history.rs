//! Prices of the last days written for the Scout addon (`History.lua`), once a day.

use anyhow::Result;
use chrono::{Local, NaiveDate};
use scout_core::addon_history::{self, HISTORY_DAYS};
use scout_core::update::AddonInstall;
use scout_sources as sources;

use super::Shared;
use crate::message::{Event, Task};

/// Starts the write when the addon is there, a house is chosen and today's file for that house is
/// not written yet (or `force`: the addon's folder was just replaced, or the user asked).
pub(super) fn start(shared: &Shared, force: bool) {
    let today = Local::now().date_naive();
    {
        let config = shared.config();
        let Ok(wow_dir) = config.wow_dir() else {
            return;
        };
        let Some(house) = config.price_server.house else {
            return;
        };
        if matches!(sources::addon_install(wow_dir), AddonInstall::Missing) {
            return;
        }
        let done = config.addon_history_day.as_deref() == Some(today.to_string().as_str())
            && config.addon_history_house.as_ref()
                == Some(&(config.price_server.region.clone(), house));
        if done && !force {
            return;
        }
    }
    shared.spawn(Task::AddonHistory, move |runtime, shared| {
        write(runtime, shared, today)
    });
}

fn write(runtime: &tokio::runtime::Runtime, shared: &Shared, today: NaiveDate) -> Result<()> {
    let (house, wow_dir) = {
        let config = shared.config();
        (
            config.price_server.auction_house(),
            config.wow_dir()?.to_path_buf(),
        )
    };
    let observations = runtime.block_on(shared.db.observations(&house))?;
    let prices = addon_history::daily_prices(&observations, today, HISTORY_DAYS);
    let first = addon_history::first_day(today, HISTORY_DAYS);
    let lua = addon_history::to_lua(&prices, first, chrono::Utc::now().timestamp());
    sources::write_history(&wow_dir, &lua)?;

    let mut config = shared.config();
    config.addon_history_day = Some(today.to_string());
    config.addon_history_house = Some((house.region.clone(), house.id));
    (shared.send)(Event::Config(config.clone()));
    config.save()?;
    (shared.send)(Event::AddonHistory {
        items: prices.len(),
        bytes: lua.len(),
    });
    Ok(())
}

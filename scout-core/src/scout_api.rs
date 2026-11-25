//! Prices from our price server (`scout-server`: the Battle.net auction API), source `scout-api`.
//!
//! Two kinds of downloads:
//! - the **latest** prices of every item, at most once an hour: kept only when they change, plus
//!   the first of each local day while the item is on sale (so every day has its price for the daily
//!   charts and the analysis), which avoids ~7 000 rows an hour;
//! - the **whole history** of one item when its market sheet opens, kept as is (a cache).

use std::collections::{HashMap, HashSet};

mod modes;

use crate::Row;
use crate::merge::SCOUT_API;
use crate::time::local_day;

/// Our price server, unless the settings name another one.
pub const DEFAULT_SERVER: &str = "https://scout.arthurdevtest.duckdns.org";
/// Latest prices and item histories are downloaded again once they are this old.
pub const REFRESH_SECS: i64 = 3600;

/// The public page of the server that thanks the data sources (`/thanks`), opened from the settings.
pub fn thanks_url(server: &str) -> String {
    let server = server.trim().trim_end_matches('/');
    let server = if server.is_empty() {
        DEFAULT_SERVER
    } else {
        server
    };
    format!("{server}/thanks")
}

/// Battle.net regions the server may record (it lists those where Forever has auction houses).
pub const REGIONS: [&str; 4] = ["eu", "us", "kr", "tw"];

/// One auction house: Battle.net region and house id; `""` / 0 = not known yet (prices recorded
/// before houses were told apart, or with none chosen).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct AuctionHouse {
    pub region: String,
    pub id: u32,
}

/// An auction house as the server lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ServerHouse {
    pub id: u32,
    /// « Hôtel des ventes de la Horde »… (five houses share each name).
    pub name: String,
    /// Newest prices of the house (Unix seconds).
    pub last_update: Option<i64>,
}

impl ServerHouse {
    /// « Alliance », « Horde » or « Flots noirs »: each group lists them in that order (seen in
    /// Europe); Blizzard's name of the house otherwise.
    pub fn side(&self) -> &str {
        const SIDES: [&str; 3] = ["Alliance", "Horde", "Flots noirs"];
        match self.id {
            110..=118 => SIDES[((self.id - 110) / 3) as usize],
            119..=121 => SIDES[(self.id - 119) as usize],
            123..=125 => SIDES[(self.id - 123) as usize],
            _ => &self.name,
        }
    }

    /// Short name: « Horde (114) ». Region and game mode are chosen (and shown) in the settings.
    pub fn label(&self) -> String {
        format!("{} ({})", self.side(), self.id)
    }
}

/// The realm type whose characters share a house: the houses come in groups of three (Alliance,
/// Horde, Flots noirs), the same ids in every region (2026-11-25); 112-115-118 belong to none of
/// them and are empty everywhere. KR and TW only have Normal and PvP realms.
pub fn realm_type(id: u32) -> Option<&'static str> {
    match id {
        110 | 113 | 116 => Some("PvP"),
        111 | 114 | 117 => Some("Normal"),
        119..=121 => Some("RP"),
        123..=125 => Some("RP-PvP"),
        _ => None,
    }
}

/// What the settings offer: the server's regions and the houses of the chosen one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerChoice {
    pub regions: Vec<String>,
    /// The region `houses` belong to.
    pub region: String,
    pub houses: Vec<ServerHouse>,
}

/// One line of the server's CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceLine {
    pub ts: i64,
    pub item_id: i64,
    /// Copper; `None` when every auction is bid-only, or the item is gone.
    pub min_unit: Option<i64>,
    /// 0 = no longer listed.
    pub quantity: i64,
}

impl PriceLine {
    /// The market's price: none when the item is gone.
    fn price(&self) -> Option<i64> {
        if self.quantity == 0 {
            None
        } else {
            self.min_unit
        }
    }

    fn row(&self, house: &AuctionHouse) -> Row {
        Row {
            region: house.region.clone(),
            house: i64::from(house.id),
            source: SCOUT_API.to_owned(),
            item_id: self.item_id,
            name: None,
            timestamp: self.ts,
            price: self.price(),
            quantity: Some(self.quantity),
        }
    }
}

/// Newest stored Scout API price of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastPrice {
    pub ts: i64,
    pub price: Option<i64>,
}

/// The rows to store from the latest prices, given the newest stored price of each item.
/// An item still priced here but missing from them left the auction house meanwhile.
pub fn latest_rows(
    house: &AuctionHouse,
    last: &HashMap<i64, LastPrice>,
    latest: &[PriceLine],
) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in latest {
        let price = line.price();
        let keep = match last.get(&line.item_id) {
            // Nothing to say about an item that was never seen on sale.
            None => price.is_some(),
            Some(before) => {
                before.price != price
                    || (price.is_some() && local_day(before.ts) != local_day(line.ts))
            }
        };
        if keep && last.get(&line.item_id).is_none_or(|b| line.ts > b.ts) {
            rows.push(line.row(house));
        }
    }
    let Some(newest) = latest.iter().map(|l| l.ts).max() else {
        return rows;
    };
    let listed: HashSet<i64> = latest.iter().map(|l| l.item_id).collect();
    let mut gone: Vec<_> = last
        .iter()
        .filter(|(id, before)| before.price.is_some() && before.ts < newest && !listed.contains(id))
        .map(|(&item_id, _)| item_id)
        .collect();
    gone.sort_unstable();
    rows.extend(gone.into_iter().map(|item_id| {
        PriceLine {
            ts: newest,
            item_id,
            min_unit: None,
            quantity: 0,
        }
        .row(house)
    }));
    rows
}

/// One item's history, stored whole.
pub fn history_rows(house: &AuctionHouse, lines: &[PriceLine]) -> Vec<Row> {
    lines.iter().map(|line| line.row(house)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: i64, item_id: i64, min_unit: Option<i64>, quantity: i64) -> PriceLine {
        PriceLine {
            ts,
            item_id,
            min_unit,
            quantity,
        }
    }

    fn eu114() -> AuctionHouse {
        AuctionHouse {
            region: "eu".into(),
            id: 114,
        }
    }

    fn kept(rows: &[Row]) -> Vec<(i64, i64, Option<i64>)> {
        rows.iter()
            .map(|r| (r.timestamp, r.item_id, r.price))
            .collect()
    }

    fn last(entries: &[(i64, i64, Option<i64>)]) -> HashMap<i64, LastPrice> {
        entries
            .iter()
            .map(|&(item_id, ts, price)| (item_id, LastPrice { ts, price }))
            .collect()
    }

    #[test]
    fn latest_keeps_changes_and_the_first_of_each_day() {
        let t = 1_000_000_000;
        let days = 86_400 * 3;
        let stored = last(&[(765, t - 3600, Some(42)), (2589, t - 3600, Some(27))]);
        let latest = [
            line(t, 765, Some(42), 90),
            line(t, 2589, Some(25), 400),
            line(t, 118, Some(8), 5),
        ];
        assert_eq!(
            kept(&latest_rows(&eu114(), &stored, &latest)),
            [(t, 2589, Some(25)), (t, 118, Some(8))]
        );
        let tomorrow = [line(t + days, 765, Some(42), 90)];
        let rows = latest_rows(&eu114(), &stored, &tomorrow);
        // Same price, new day: kept; 2589 is missing from the listing: it left.
        assert_eq!(
            kept(&rows),
            [(t + days, 765, Some(42)), (t + days, 2589, None)]
        );
        assert_eq!(rows[0].source, SCOUT_API);
        assert_eq!((rows[0].region.as_str(), rows[0].house), ("eu", 114));
        assert_eq!(rows[1].quantity, Some(0));
    }

    #[test]
    fn gone_items_are_written_once_and_old_lines_never() {
        let t = 1_000_000_000;
        let stored = last(&[(7, t - 3600, None), (9, t + 60, Some(5))]);
        let latest = [
            line(t, 7, None, 0),
            line(t, 8, None, 0),
            line(t, 9, Some(4), 1),
        ];
        // 7 already gone, 8 never seen, 9 has a newer price stored (from its history).
        assert!(latest_rows(&eu114(), &stored, &latest).is_empty());
        assert!(latest_rows(&eu114(), &stored, &[]).is_empty());
    }

    #[test]
    fn houses_are_named_by_side_and_group() {
        let house = |id, name: &str| ServerHouse {
            id,
            name: name.into(),
            last_update: None,
        };
        assert_eq!(
            house(114, "Hôtel des ventes de la Horde").label(),
            "Horde (114)"
        );
        assert_eq!(
            house(116, "Hôtel des ventes des Flots noirs").label(),
            "Flots noirs (116)"
        );
        assert_eq!(
            house(124, "Hôtel des ventes de la Horde").label(),
            "Horde (124)"
        );
        assert_eq!(house(200, "Auction House").label(), "Auction House (200)");
        assert_eq!(
            house(115, "Hôtel des ventes de la Horde").label(),
            "Horde (115)"
        );
    }

    #[test]
    fn history_is_kept_whole() {
        let lines = [line(100, 765, Some(42), 9), line(200, 765, Some(41), 0)];
        assert_eq!(
            kept(&history_rows(&eu114(), &lines)),
            [(100, 765, Some(42)), (200, 765, None)]
        );
    }

    #[test]
    fn thanks_page_is_on_the_chosen_server() {
        assert_eq!(
            thanks_url("http://localhost:3000/"),
            "http://localhost:3000/thanks"
        );
        assert_eq!(thanks_url(" "), format!("{DEFAULT_SERVER}/thanks"));
    }
}

//! What really sells: two consecutive versions of one listing compared auction by auction.
//!
//! An auction is followed by its `id`. A quantity that drops is a proven sale; an auction that
//! vanished while it still had hours left was bought or cancelled (an estimate); one that vanished
//! near its end most likely expired and is ignored.
//!
//! Memory: the older version is read into a compact table (`id -> Kept`), the newer one is streamed
//! through it once; neither list is ever held whole. Both are read through `Read`, so a snapshot
//! file is decompressed on the fly.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// Two versions further apart than this are not compared: too many auctions could have been posted,
/// sold and expired in between, and the `time_left` of the old one says too little. Blizzard's
/// `LONG` is 2 to 12 hours left, so within two hours a `LONG` auction cannot have expired.
pub const MAX_GAP: i64 = 2 * 3600;

/// One row of `sales`, for one item between two versions (summed per day by the database).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ItemSales {
    pub item_id: u32,
    /// Units whose auction lost quantity but is still listed (partial purchase).
    pub sold_proven: u64,
    /// Units of auctions with hours left that vanished (bought, or cancelled).
    pub sold_estimated: u64,
    /// Sum of units x unit price (copper) over the sales that have a price; unpriced ones add nothing.
    pub sold_value: u64,
}

/// What is kept of an auction of the older version.
struct Kept {
    item: u32,
    quantity: u64,
    /// `LONG` or `VERY_LONG`.
    long: bool,
    /// Unit price; `None` without a buyout.
    unit: Option<u64>,
}

#[derive(Deserialize)]
struct Auction {
    id: u64,
    item: Item,
    quantity: u64,
    /// Commodities.
    unit_price: Option<u64>,
    /// Other items: the price of the whole lot.
    buyout: Option<u64>,
    time_left: Option<String>,
}

#[derive(Deserialize)]
struct Item {
    id: u32,
}

/// Calls `each` for every auction of a listing, one at a time.
fn for_each_auction(reader: impl Read, mut each: impl FnMut(Auction)) -> Result<()> {
    let mut de = serde_json::Deserializer::from_reader(reader);
    Root(&mut each)
        .deserialize(&mut de)
        .context("enchères illisibles")
}

/// The listing object: only its `auctions` array matters (an empty house has none).
struct Root<'a, F>(&'a mut F);

impl<'de, F: FnMut(Auction)> DeserializeSeed<'de> for Root<'_, F> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de, F: FnMut(Auction)> Visitor<'de> for Root<'_, F> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a listing")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "auctions" {
                map.next_value_seed(Auctions(&mut *self.0))?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}

struct Auctions<'a, F>(&'a mut F);

impl<'de, F: FnMut(Auction)> DeserializeSeed<'de> for Auctions<'_, F> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de, F: FnMut(Auction)> Visitor<'de> for Auctions<'_, F> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of auctions")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(auction) = seq.next_element::<Auction>()? {
            (self.0)(auction);
        }
        Ok(())
    }
}

/// Unit price of an auction, `None` when it has no immediate purchase price.
fn unit_price(a: &Auction, quantity: u64) -> Option<u64> {
    a.unit_price.or(a.buyout.map(|b| b / quantity))
}

/// The sales between `older` and `newer`, by item (items without any sale are left out).
pub fn compare(older: impl Read, newer: impl Read) -> Result<Vec<ItemSales>> {
    let mut gone: HashMap<u64, Kept> = HashMap::new();
    for_each_auction(older, |a| {
        let quantity = a.quantity.max(1);
        let long = matches!(a.time_left.as_deref(), Some("LONG" | "VERY_LONG"));
        let unit = unit_price(&a, quantity);
        let kept = Kept {
            item: a.item.id,
            quantity,
            long,
            unit,
        };
        gone.insert(a.id, kept);
    })?;

    let mut items: BTreeMap<u32, ItemSales> = BTreeMap::new();
    let mut sell = |kept: &Kept, units: u64, proven: bool| {
        let sales = items.entry(kept.item).or_default();
        if proven {
            sales.sold_proven += units;
        } else {
            sales.sold_estimated += units;
        }
        sales.sold_value += units * kept.unit.unwrap_or(0);
    };
    // What is still listed leaves `gone`; what stays in it has vanished.
    for_each_auction(newer, |a| {
        if let Some(kept) = gone.remove(&a.id) {
            let quantity = a.quantity.max(1);
            if quantity < kept.quantity {
                sell(&kept, kept.quantity - quantity, true);
            }
        }
    })?;
    for kept in gone.values().filter(|k| k.long) {
        sell(kept, kept.quantity, false);
    }
    Ok(items
        .into_iter()
        .map(|(item_id, s)| ItemSales { item_id, ..s })
        .collect())
}

/// The sales since the older snapshot `(fetched_at, file)`, given the newer one's time and body.
///
/// Empty without an older snapshot, when its file is gone, or when the gap exceeds `MAX_GAP`.
pub fn since_previous(
    previous: Option<(i64, PathBuf)>,
    ts: i64,
    newer: impl Read,
) -> Result<Vec<ItemSales>> {
    let Some((_, path)) = previous.filter(|(at, _)| ts - at <= MAX_GAP) else {
        return Ok(Vec::new());
    };
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("lecture de {}", path.display())),
    };
    let older = zstd::stream::read::Decoder::new(BufReader::new(file)).context("décompression")?;
    compare(older, newer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(auctions: &[&str]) -> Vec<u8> {
        format!(r#"{{"_links":{{}},"auctions":[{}]}}"#, auctions.join(",")).into_bytes()
    }

    fn lot(id: u64, item: u32, quantity: u64, price: &str, left: &str) -> String {
        format!(
            r#"{{"id":{id},"item":{{"id":{item}}},{price},"quantity":{quantity},"time_left":"{left}"}}"#
        )
    }

    fn run(older: &[&str], newer: &[&str]) -> Vec<ItemSales> {
        compare(&listing(older)[..], &listing(newer)[..]).unwrap()
    }

    #[test]
    fn partial_purchase_is_proven() {
        let before = lot(1, 118, 10, r#""unit_price":5"#, "SHORT");
        let after = lot(1, 118, 7, r#""unit_price":5"#, "SHORT");
        let sales = run(&[&before], &[&after]);
        let expected = ItemSales {
            item_id: 118,
            sold_proven: 3,
            sold_estimated: 0,
            sold_value: 15,
        };
        assert_eq!(sales, [expected]);
    }

    #[test]
    fn long_auction_that_vanishes_is_estimated() {
        let long = lot(1, 768, 2, r#""buyout":300"#, "VERY_LONG");
        let kept = lot(2, 768, 1, r#""buyout":100"#, "LONG");
        let sales = run(&[&long, &kept], &[&kept]);
        let expected = ItemSales {
            item_id: 768,
            sold_proven: 0,
            sold_estimated: 2,
            sold_value: 300,
        };
        assert_eq!(sales, [expected]);
    }

    #[test]
    fn short_auction_that_vanishes_is_ignored() {
        let short = lot(1, 768, 1, r#""buyout":300"#, "SHORT");
        let medium = lot(2, 769, 1, r#""buyout":300"#, "MEDIUM");
        assert!(run(&[&short, &medium], &[]).is_empty());
        // An empty house has no `auctions` field.
        let empty = br#"{"_links":{}}"#;
        assert!(
            compare(&listing(&[&short])[..], &empty[..])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn auction_without_buyout_counts_without_price() {
        let bid_only = lot(1, 7, 1, r#""bid":50"#, "LONG");
        let sales = run(&[&bid_only], &[]);
        assert_eq!((sales[0].sold_estimated, sales[0].sold_value), (1, 0));
    }

    #[test]
    fn new_auctions_and_unchanged_ones_sell_nothing() {
        let same = lot(1, 118, 4, r#""unit_price":5"#, "LONG");
        let new = lot(2, 118, 4, r#""unit_price":5"#, "LONG");
        assert!(run(&[&same], &[&same, &new]).is_empty());
    }

    #[test]
    fn gap_too_large_is_not_compared() {
        let long = lot(1, 768, 1, r#""buyout":300"#, "LONG");
        let dir = std::env::temp_dir().join(format!("scout-sales-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.json.zst");
        std::fs::write(&path, zstd::encode_all(&listing(&[&long])[..], 1).unwrap()).unwrap();
        let newer = listing(&[]);
        let near = since_previous(Some((1000, path.clone())), 1000 + MAX_GAP, &newer[..]);
        assert_eq!(near.unwrap().len(), 1);
        let far = since_previous(Some((1000, path.clone())), 1001 + MAX_GAP, &newer[..]);
        assert!(far.unwrap().is_empty());
        let missing = since_previous(Some((1000, dir.join("none"))), 1100, &newer[..]);
        assert!(missing.unwrap().is_empty());
        assert!(since_previous(None, 1100, &newer[..]).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

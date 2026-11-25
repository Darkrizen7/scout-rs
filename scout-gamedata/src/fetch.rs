//! A whole download: the client tables of a build, assembled, plus the recipe items they lack, named
//! by Wowhead (and any other item the caller wants named).

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use scout_core::gamedata::{GameItem, Tables};

use crate::{wago, wowhead};

/// What a download gives.
pub struct Download {
    pub tables: Tables,
    /// Why Wowhead stopped answering (403…), when it did: the tables lack some items, and the next
    /// download should try again.
    pub blocked: Option<String>,
    /// The caller's limit stopped the naming before the end: items remain to be asked.
    pub limited: bool,
}

/// How a naming round ended.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Naming {
    /// Why Wowhead refused (403…), when it did.
    pub blocked: Option<String>,
    /// The limit of requests was reached with items still to ask (not a refusal).
    pub limited: bool,
}

impl Naming {
    /// Every wanted item was asked for.
    pub fn complete(&self) -> bool {
        self.blocked.is_none() && !self.limited
    }
}

/// The client tables of `build`, plus the recipe items they lack and the `extra` items, named by
/// Wowhead. `known` holds the items Wowhead already named and `unknown` those it does not know
/// (each id is asked for once; new answers are added to both, kept by the caller).
/// `limit`: at most that many Wowhead requests (None: no limit, as in the app).
/// `progress` receives a French line at each step.
pub fn download(
    build: &str,
    extra: &[i64],
    known: &mut HashMap<i64, GameItem>,
    unknown: &mut HashSet<i64>,
    limit: Option<usize>,
    progress: &dyn Fn(String),
) -> Result<Download> {
    let mut csv = HashMap::new();
    for (i, table) in wago::TABLES.iter().enumerate() {
        progress(format!(
            "Forever {build} : téléchargement {}/{} ({table})…",
            i + 1,
            wago::TABLES.len()
        ));
        csv.insert(*table, wago::table_csv(table, build)?);
    }
    progress("Assemblage des recettes…".into());
    let mut tables = wago::assemble(&csv)?;
    let Naming { blocked, limited } =
        name_items(&mut tables, extra, known, unknown, limit, progress);
    Ok(Download {
        tables,
        blocked,
        limited,
    })
}

/// Items to name: recipe items missing from the client tables plus `extra`, minus those the tables
/// already have and those Wowhead does not know (sorted, no duplicate).
pub fn wanted_items(tables: &Tables, extra: &[i64], unknown: &HashSet<i64>) -> Vec<i64> {
    let named: HashSet<i64> = tables.items.iter().map(|i| i.id).collect();
    let mut wanted: Vec<i64> = tables
        .missing_recipe_items()
        .into_iter()
        .chain(extra.iter().copied())
        .filter(|id| !named.contains(id) && !unknown.contains(id))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted
}

/// Adds to `tables` the items it lacks (recipe items and `extra`), named by Wowhead. Stops at the
/// first refusal (403…), or after `limit` requests (None: no limit); the result says which.
pub fn name_items(
    tables: &mut Tables,
    extra: &[i64],
    known: &mut HashMap<i64, GameItem>,
    unknown: &mut HashSet<i64>,
    limit: Option<usize>,
    progress: &dyn Fn(String),
) -> Naming {
    let mut naming = Naming::default();
    let wanted = wanted_items(tables, extra, unknown);
    let to_ask: Vec<i64> = wanted
        .iter()
        .copied()
        .filter(|id| !known.contains_key(id))
        .collect();
    for (i, &id) in to_ask.iter().enumerate() {
        if limit.is_some_and(|max| i >= max) {
            naming.limited = true;
            break;
        }
        if i > 0 {
            std::thread::sleep(wowhead::PAUSE);
        }
        progress(format!(
            "Objets absents des tables du jeu, depuis Wowhead : {}/{}…",
            i + 1,
            to_ask.len()
        ));
        // Wowhead only names a few extra items: when it refuses, keep the client tables rather
        // than losing the whole download.
        match wowhead::item(id) {
            Ok(Some(item)) => {
                known.insert(id, item);
            }
            // Remembered too, or it would be asked again at every round.
            Ok(None) => {
                unknown.insert(id);
            }
            Err(error) => {
                naming.blocked = Some(format!("{error:#}"));
                break;
            }
        }
    }
    tables.add_items(wanted.iter().filter_map(|id| known.get(id).cloned()));
    naming
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64) -> GameItem {
        GameItem {
            id,
            name: String::new(),
            quality: 0,
            item_level: 0,
            required_level: 0,
            class_id: None,
            subclass_id: None,
            buy_price: 0,
            vendor_stack: 1,
            sell_price: 0,
        }
    }

    #[test]
    fn limit_stops_without_a_refusal() {
        let mut tables = Tables::default();
        let mut known: HashMap<i64, GameItem> = (1..=3).map(|id| (id, item(id))).collect();
        let mut unknown = HashSet::new();
        // Everything is already known: no request, so a zero limit is not reached.
        let done = name_items(
            &mut tables,
            &[1, 2, 3],
            &mut known,
            &mut unknown,
            Some(0),
            &|_| {},
        );
        assert!(done.complete());
        assert_eq!(tables.items.len(), 3);
        // Nothing known, limit 0: the first item is not asked, the round is limited.
        let mut tables = Tables::default();
        let done = name_items(
            &mut tables,
            &[7],
            &mut HashMap::new(),
            &mut unknown,
            Some(0),
            &|_| {},
        );
        assert_eq!(
            done,
            Naming {
                blocked: None,
                limited: true
            }
        );
    }

    #[test]
    fn wanted_skips_named_and_unknown_items() {
        let mut tables = Tables::default();
        tables.items.push(item(2));
        let unknown = HashSet::from([4]);
        assert_eq!(
            wanted_items(&tables, &[5, 3, 2, 4, 3], &unknown),
            vec![3, 5]
        );
    }
}

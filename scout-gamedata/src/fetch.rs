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
}

/// The client tables of `build`, plus the recipe items they lack and the `extra` items, named by
/// Wowhead. `known` holds the items Wowhead already named and `unknown` those it does not know
/// (each id is asked for once; new answers are added to both, kept by the caller).
/// `progress` receives a French line at each step.
pub fn download(
    build: &str,
    extra: &[i64],
    known: &mut HashMap<i64, GameItem>,
    unknown: &mut HashSet<i64>,
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
    let blocked = name_items(&mut tables, extra, known, unknown, progress);
    Ok(Download { tables, blocked })
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
/// first refusal (403…) and returns why.
pub fn name_items(
    tables: &mut Tables,
    extra: &[i64],
    known: &mut HashMap<i64, GameItem>,
    unknown: &mut HashSet<i64>,
    progress: &dyn Fn(String),
) -> Option<String> {
    let mut blocked = None;
    let wanted = wanted_items(tables, extra, unknown);
    let to_ask: Vec<i64> = wanted
        .iter()
        .copied()
        .filter(|id| !known.contains_key(id))
        .collect();
    for (i, &id) in to_ask.iter().enumerate() {
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
                blocked = Some(format!("{error:#}"));
                break;
            }
        }
    }
    tables.add_items(wanted.iter().filter_map(|id| known.get(id).cloned()));
    blocked
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

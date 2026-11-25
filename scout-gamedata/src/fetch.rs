//! A whole download: the client tables of a build, assembled, plus the recipe items they lack, named
//! by Wowhead.

use std::collections::HashMap;

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

/// The client tables of `build` and the recipe items they lack. `known` holds the items Wowhead
/// already named (each is asked for once; the new answers are added to it, kept by the caller).
/// `progress` receives a French line at each step.
pub fn download(
    build: &str,
    known: &mut HashMap<i64, GameItem>,
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
    let blocked = add_wowhead_items(&mut tables, known, progress);
    Ok(Download { tables, blocked })
}

/// Recipe items missing from the client tables (sent by Forever's server as hotfixes), named by
/// Wowhead. Stops at the first refusal (403…) and returns why.
fn add_wowhead_items(
    tables: &mut Tables,
    known: &mut HashMap<i64, GameItem>,
    progress: &dyn Fn(String),
) -> Option<String> {
    let mut blocked = None;
    let missing = tables.missing_recipe_items();
    let unknown: Vec<i64> = missing
        .iter()
        .copied()
        .filter(|id| !known.contains_key(id))
        .collect();
    for (i, &id) in unknown.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(wowhead::PAUSE);
        }
        progress(format!(
            "Objets absents des tables du jeu, depuis Wowhead : {}/{}…",
            i + 1,
            unknown.len()
        ));
        // Wowhead only names a few extra items: when it refuses, keep the client tables rather
        // than losing the whole download.
        match wowhead::item(id) {
            Ok(Some(item)) => {
                known.insert(id, item);
            }
            Ok(None) => {}
            Err(error) => {
                blocked = Some(format!("{error:#}"));
                break;
            }
        }
    }
    tables.add_items(missing.iter().filter_map(|id| known.get(id).cloned()));
    blocked
}

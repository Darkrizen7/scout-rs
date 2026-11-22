//! Continents and zones: names and parents from UiMap, world rectangle from UiMapAssignment.

use std::collections::HashMap;

use anyhow::Result;
use csv::StringRecord;

use super::{Table, int, text};
use scout_core::map::{Faction, GameMap, name_key};

fn float(row: &StringRecord, column: usize) -> f64 {
    row.get(column).and_then(|t| t.parse().ok()).unwrap_or(0.0)
}

/// Maps with the world area of their whole extent (assignments covering 0..1 of the map; a map with several,
/// like the world map, is left out).
pub(super) fn maps(ui_map: &Table, assignment: &Table) -> Result<Vec<GameMap>> {
    let [map_id, u0, v0, u1, v1] =
        ["UiMapID", "UiMin_0", "UiMin_1", "UiMax_0", "UiMax_1"].map(|c| assignment.column(c));
    let [map_id, u0, v0, u1, v1] = [map_id?, u0?, v0?, u1?, v1?];
    let region = |i: usize| assignment.column(&format!("Region_{i}"));
    let [x0, y0, x1, y1] = [region(0)?, region(1)?, region(3)?, region(4)?];
    let mut areas: HashMap<i64, [f64; 4]> = HashMap::new();
    for r in &assignment.rows {
        let whole = [float(r, u0), float(r, v0)] == [0.0, 0.0]
            && [float(r, u1), float(r, v1)] == [1.0, 1.0];
        if whole {
            areas.entry(int(r, map_id)).or_insert([
                float(r, x0),
                float(r, x1),
                float(r, y0),
                float(r, y1),
            ]);
        }
    }
    let [id, name, parent, kind] =
        ["ID", "Name_lang", "ParentUiMapID", "Type"].map(|c| ui_map.column(c));
    let [id, name, parent, kind] = [id?, name?, parent?, kind?];
    let mut maps: Vec<GameMap> = ui_map
        .rows
        .iter()
        .filter_map(|r| {
            let [x_min, x_max, y_min, y_max] = *areas.get(&int(r, id))?;
            Some(GameMap {
                id: int(r, id),
                name: text(r, name),
                parent: int(r, parent),
                kind: int(r, kind),
                x_min,
                x_max,
                y_min,
                y_max,
            })
        })
        .collect();
    maps.sort_by_key(|m| m.id);
    Ok(maps)
}

/// Factions of the zones: AreaTable's top-level areas (no parent) that belong to Alliance or Horde, by name.
pub(super) fn factions(area: &Table) -> Result<HashMap<String, Faction>> {
    let [name, parent, mask] =
        ["AreaName_lang", "ParentAreaID", "FactionGroupMask"].map(|c| area.column(c));
    let [name, parent, mask] = [name?, parent?, mask?];
    Ok(area
        .rows
        .iter()
        .filter(|r| int(r, parent) == 0)
        .filter_map(|r| Some((name_key(&text(r, name)), Faction::from_mask(int(r, mask))?)))
        .collect())
}

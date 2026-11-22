//! Loot by map zone: the monsters of a zone or dungeon and everything each one drops, Forever Guide's chance
//! next to what Scout recorded.

use super::{DropKind, DropNpc, Observed, TRUSTED_KILLS};

/// A zone or dungeon with monsters whose loot is known.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub name: String,
    pub dungeon: bool,
    pub npcs: i64,
    /// Map (UiMap id) of the zone, None for a dungeon or a zone only Scout knows.
    pub ui_map: Option<i64>,
    /// Usual monster levels: 10th and 90th percentiles (a few odd NPCs, like a level 1 critter or a
    /// level 62 guard, would stretch a plain min / max).
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    /// Corpses of this zone looted by the user (Scout).
    pub kills: i64,
}

/// A monster and its zone, as the database returns it, to sum up the zones.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ZoneNpcLevel {
    pub zone: String,
    pub dungeon: bool,
    pub ui_map: Option<i64>,
    pub level: Option<i64>,
    pub kills: Option<i64>,
}

/// One entry per zone name, sorted by name.
pub fn zones(rows: Vec<ZoneNpcLevel>) -> Vec<Zone> {
    let mut by_name: std::collections::BTreeMap<String, (Zone, Vec<i64>)> = Default::default();
    for r in rows {
        let (zone, levels) = by_name.entry(r.zone.clone()).or_insert_with(|| {
            let zone = Zone {
                name: r.zone,
                dungeon: false,
                npcs: 0,
                ui_map: None,
                min_level: None,
                max_level: None,
                kills: 0,
            };
            (zone, Vec::new())
        });
        zone.dungeon |= r.dungeon;
        zone.npcs += 1;
        zone.ui_map = zone.ui_map.max(r.ui_map);
        zone.kills += r.kills.unwrap_or(0);
        levels.extend(r.level);
    }
    let mut zones: Vec<Zone> = by_name
        .into_values()
        .map(|(mut zone, mut levels)| {
            levels.sort_unstable();
            let rank = |p: f64| levels.get(((levels.len() - 1) as f64 * p).round() as usize);
            if !levels.is_empty() {
                zone.min_level = rank(0.1).copied();
                zone.max_level = rank(0.9).copied();
            }
            zone
        })
        .collect();
    zones.sort_by_key(|z| z.name.to_lowercase());
    zones
}

/// A monster of the zone, as the database returns it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ZoneNpcRow {
    pub npc_id: i64,
    pub name: Option<String>,
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    pub rank: Option<i64>,
    pub zone: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub dungeon: Option<String>,
    pub ui_map: Option<i64>,
    pub kills: Option<i64>,
    pub skins: Option<i64>,
}

/// A reference drop of a monster of the zone (`kind`: npc, skinning or pickpocket).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ZoneDropRow {
    pub npc_id: i64,
    pub kind: String,
    pub item_id: i64,
    pub chance: Option<f64>,
    pub quest_only: bool,
}

/// What Scout recorded for a monster of the zone (`kind`: loot or skin).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ZoneObservedRow {
    pub npc_id: i64,
    pub kind: String,
    pub item_id: i64,
    pub drops: i64,
    pub quantity: i64,
}

/// One item a monster gives.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcDrop {
    pub kind: DropKind,
    pub item_id: i64,
    /// Forever Guide's chance (percent), None when unknown.
    pub chance: Option<f64>,
    pub quest_only: bool,
    pub observed: Option<Observed>,
}

impl NpcDrop {
    /// Chance used for estimates and sorting: the user's rate once trusted (or when it is all there is),
    /// otherwise Forever Guide's.
    pub fn best_chance(&self) -> Option<f64> {
        match (self.observed, self.chance) {
            (Some(o), _) if o.total >= TRUSTED_KILLS => Some(o.rate()),
            (_, Some(c)) => Some(c),
            (Some(o), None) => Some(o.rate()),
            (None, None) => None,
        }
    }

    /// Expected units per corpse: chance × average quantity seen (1 when never seen).
    pub fn per_kill(&self) -> Option<f64> {
        let quantity = match self.observed {
            Some(o) if o.drops > 0 => o.average_quantity(),
            _ => 1.0,
        };
        Some(self.best_chance()? / 100.0 * quantity)
    }
}

/// A monster and its loot, sorted by kind then best chance.
#[derive(Debug, Clone)]
pub struct ZoneNpc {
    pub npc: DropNpc,
    /// Corpses looted / skinned by the user.
    pub kills: i64,
    pub skins: i64,
    pub drops: Vec<NpcDrop>,
}

impl ZoneNpc {
    pub fn name(&self) -> String {
        self.npc
            .name
            .clone()
            .unwrap_or_else(|| format!("n° {}", self.npc.npc_id))
    }

    /// Value of a looted corpse (skinning and pickpocketing aside) from a unit price; None when nothing it
    /// drops has a price. Items only dropped during a quest are left out.
    pub fn value_per_kill(&self, price: impl Fn(i64) -> Option<f64>) -> Option<f64> {
        self.drops
            .iter()
            .filter(|d| d.kind == DropKind::Npc && !d.quest_only)
            .filter_map(|d| Some(d.per_kill()? * price(d.item_id)?))
            .reduce(|a, b| a + b)
    }

    pub fn of(&self, kind: DropKind) -> impl Iterator<Item = &NpcDrop> {
        self.drops.iter().filter(move |d| d.kind == kind)
    }
}

/// The monsters of a zone, by level then name.
#[derive(Debug, Clone, Default)]
pub struct ZoneLoot {
    pub zone: String,
    pub npcs: Vec<ZoneNpc>,
}

impl ZoneLoot {
    pub fn new(
        zone: String,
        npcs: Vec<ZoneNpcRow>,
        drops: Vec<ZoneDropRow>,
        observed: Vec<ZoneObservedRow>,
    ) -> Self {
        let mut npcs: Vec<ZoneNpc> = npcs
            .into_iter()
            .map(|r| ZoneNpc {
                kills: r.kills.unwrap_or(0),
                skins: r.skins.unwrap_or(0),
                drops: Vec::new(),
                npc: DropNpc {
                    npc_id: r.npc_id,
                    name: r.name,
                    min_level: r.min_level,
                    max_level: r.max_level,
                    rank: r.rank,
                    zone: r.zone,
                    x: r.x,
                    y: r.y,
                    dungeon: r.dungeon,
                    ui_map: r.ui_map,
                },
            })
            .collect();
        npcs.sort_by_key(|n| n.npc.npc_id);
        let find = |npcs: &[ZoneNpc], id| npcs.binary_search_by_key(&id, |n| n.npc.npc_id).ok();
        for d in drops {
            let Some(kind) = DropKind::parse(&d.kind) else {
                continue;
            };
            let Some(i) = find(&npcs, d.npc_id) else {
                continue;
            };
            let npc = &mut npcs[i];
            // Killed but never seen dropping: 0 out of the corpses.
            let total = match kind {
                DropKind::Npc => npc.kills,
                DropKind::Skinning => npc.skins,
                _ => 0,
            };
            npc.drops.push(NpcDrop {
                kind,
                item_id: d.item_id,
                chance: d.chance,
                quest_only: d.quest_only,
                observed: (total > 0).then_some(Observed {
                    drops: 0,
                    total,
                    quantity: 0,
                }),
            });
        }
        for o in observed {
            let kind = match o.kind.as_str() {
                "loot" => DropKind::Npc,
                "skin" => DropKind::Skinning,
                _ => continue,
            };
            let Some(i) = find(&npcs, o.npc_id) else {
                continue;
            };
            let npc = &mut npcs[i];
            let seen = Observed {
                drops: o.drops,
                total: if kind == DropKind::Npc {
                    npc.kills
                } else {
                    npc.skins
                },
                quantity: o.quantity,
            };
            match npc
                .drops
                .iter_mut()
                .find(|d| d.kind == kind && d.item_id == o.item_id)
            {
                Some(d) => d.observed = Some(seen),
                // Seen in game but unknown to the reference (e.g. added by Forever).
                None => npc.drops.push(NpcDrop {
                    kind,
                    item_id: o.item_id,
                    chance: None,
                    quest_only: false,
                    observed: Some(seen),
                }),
            }
        }
        for npc in &mut npcs {
            npc.drops.sort_by(|a, b| {
                a.kind.cmp(&b.kind).then_with(|| {
                    // Known chances first, highest first.
                    let key = |d: &NpcDrop| d.best_chance().map(|c| -c);
                    match (key(a), key(b)) {
                        (Some(x), Some(y)) => x.total_cmp(&y),
                        (x, y) => y.is_some().cmp(&x.is_some()),
                    }
                    .then(a.item_id.cmp(&b.item_id))
                })
            });
        }
        npcs.sort_by(|a, b| {
            let level = |n: &ZoneNpc| n.npc.min_level.or(n.npc.max_level).unwrap_or(i64::MAX);
            level(a)
                .cmp(&level(b))
                .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
        });
        Self { zone, npcs }
    }
}

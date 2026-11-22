//! Where an item comes from: Forever Guide's reference data (monsters, gathering, chests, containers, quests)
//! merged with the loot Scout recorded in game (kills and drops of the user).

use serde::Deserialize;

/// Kills (or skinned corpses) from which the observed rate is considered reliable (shown highlighted).
pub const TRUSTED_KILLS: i64 = 20;
/// z for a 95 % confidence interval.
const Z95: f64 = 1.96;

/// How the item is obtained, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DropKind {
    Npc,
    Skinning,
    Pickpocket,
    Object,
    Container,
    Quest,
    QuestChoice,
}

impl DropKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "npc" => Self::Npc,
            "skinning" => Self::Skinning,
            "pickpocket" => Self::Pickpocket,
            "object" => Self::Object,
            "container" => Self::Container,
            "quest" => Self::Quest,
            "quest_choice" => Self::QuestChoice,
            _ => return None,
        })
    }

    /// Section title in the item sheet.
    pub fn title(self) -> &'static str {
        match self {
            Self::Npc => "Monstres",
            Self::Skinning => "Dépeçage",
            Self::Pickpocket => "Vol à la tire",
            Self::Object => "Objets du monde (coffres, filons…)",
            Self::Container => "Contenu dans",
            Self::Quest => "Récompense de quête",
            Self::QuestChoice => "Récompense de quête (au choix)",
        }
    }

    /// Sources that are NPCs (levels, zone, kills).
    pub fn is_npc(self) -> bool {
        matches!(self, Self::Npc | Self::Skinning | Self::Pickpocket)
    }
}

/// `drops_export.lua npcs` line.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DropNpc {
    pub npc_id: i64,
    pub name: Option<String>,
    pub min_level: Option<i64>,
    pub max_level: Option<i64>,
    /// 0 normal, 1 elite, 2 rare elite, 3 boss, 4 rare (creature_template rank).
    pub rank: Option<i64>,
    pub zone: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub dungeon: Option<String>,
    /// Map (UiMap id) of its zone, the one `x` and `y` are on.
    #[serde(default)]
    pub ui_map: Option<i64>,
}

/// The user's own results for a source: `drops` corpses out of `total` gave the item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observed {
    pub drops: i64,
    pub total: i64,
    pub quantity: i64,
}

impl Observed {
    /// Percent of corpses that gave the item.
    pub fn rate(&self) -> f64 {
        100.0 * self.drops as f64 / self.total.max(1) as f64
    }

    /// Wilson 95 % interval of the rate, in percent.
    pub fn interval(&self) -> (f64, f64) {
        let n = self.total.max(1) as f64;
        let p = self.drops as f64 / n;
        let z2 = Z95 * Z95;
        let center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
        let half = Z95 / (1.0 + z2 / n) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
        (
            100.0 * (center - half).max(0.0),
            100.0 * (center + half).min(1.0),
        )
    }

    /// Items per corpse that gave it.
    pub fn average_quantity(&self) -> f64 {
        self.quantity as f64 / self.drops.max(1) as f64
    }
}

#[derive(Debug, Clone)]
pub struct Drop {
    pub kind: DropKind,
    pub source_id: i64,
    pub name: String,
    /// Reference chance (percent), None when unknown.
    pub chance: Option<f64>,
    pub quest_only: bool,
    pub observed: Option<Observed>,
    pub npc: Option<DropNpc>,
}

/// All sources of one item, sorted, and how many more Forever Guide did not list.
#[derive(Debug, Clone, Default)]
pub struct ItemDrops {
    pub drops: Vec<Drop>,
    pub more: Vec<(DropKind, i64)>,
    /// Corpses Scout recorded overall (any monster).
    pub total_kills: i64,
}

impl ItemDrops {
    /// Merges the reference sources and the user's loot; sorted by kind, then reference chance (unknown last).
    pub fn new(base: Vec<BaseRow>, observed: Vec<ObservedRow>, total_kills: i64) -> Self {
        let mut drops = Vec::with_capacity(base.len());
        let mut more = Vec::new();
        for row in base {
            let Some(kind) = DropKind::parse(&row.kind) else {
                continue;
            };
            if row.source_id == 0 {
                more.push((kind, row.more));
                continue;
            }
            // Killed but never seen dropping: 0 out of `kills`.
            let total = match kind {
                DropKind::Npc => row.kills,
                DropKind::Skinning => row.skins,
                _ => None,
            };
            drops.push(Drop {
                kind,
                source_id: row.source_id,
                name: row
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("n° {}", row.source_id)),
                chance: row.chance,
                quest_only: row.quest_only,
                observed: total.filter(|&t| t > 0).map(|total| Observed {
                    drops: 0,
                    total,
                    quantity: 0,
                }),
                npc: kind.is_npc().then_some(DropNpc {
                    npc_id: row.source_id,
                    name: row.name,
                    min_level: row.min_level,
                    max_level: row.max_level,
                    rank: row.rank,
                    zone: row.zone,
                    x: row.x,
                    y: row.y,
                    dungeon: row.dungeon,
                    ui_map: row.ui_map,
                }),
            });
        }
        for o in observed {
            let (kind, total) = match o.kind.as_str() {
                "loot" => (DropKind::Npc, o.kills),
                "skin" => (DropKind::Skinning, o.skins),
                _ => continue,
            };
            let seen = Observed {
                drops: o.drops,
                total,
                quantity: o.quantity,
            };
            match drops
                .iter_mut()
                .find(|d| d.kind == kind && d.source_id == o.npc_id)
            {
                Some(d) => d.observed = Some(seen),
                // A monster the reference does not know for this item (e.g. added by Forever).
                None => drops.push(Drop {
                    kind,
                    source_id: o.npc_id,
                    name: o.name.clone().unwrap_or_else(|| format!("n° {}", o.npc_id)),
                    chance: None,
                    quest_only: false,
                    observed: Some(seen),
                    npc: Some(DropNpc {
                        npc_id: o.npc_id,
                        name: o.name,
                        zone: o.zone,
                        ..DropNpc::default()
                    }),
                }),
            }
        }
        drops.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| compare(a, b, DropColumn::Chance, true))
        });
        more.sort();
        Self {
            drops,
            more,
            total_kills,
        }
    }

    /// Sources of one kind, in order.
    pub fn of(&self, kind: DropKind) -> impl Iterator<Item = &Drop> {
        self.drops.iter().filter(move |d| d.kind == kind)
    }
}

impl DropNpc {
    /// Neither elite, rare nor boss (unknown rank counts as normal).
    pub fn is_normal(&self) -> bool {
        matches!(self.rank, None | Some(0))
    }

    /// Elite or rare elite (creature classification 1 and 2).
    pub fn is_elite(&self) -> bool {
        matches!(self.rank, Some(1 | 2))
    }

    /// Rare or rare elite (classification 4 and 2).
    pub fn is_rare(&self) -> bool {
        matches!(self.rank, Some(2 | 4))
    }

    pub fn is_boss(&self) -> bool {
        self.rank == Some(3)
    }

    /// "24" or "24-25", without the rank.
    pub fn level_range(&self) -> String {
        match (self.min_level, self.max_level) {
            (Some(a), Some(b)) if a != b => format!("{a}-{b}"),
            (Some(a), _) | (None, Some(a)) => a.to_string(),
            (None, None) => String::new(),
        }
    }

    /// "24", "24-25", with "élite" / "rare" / "boss".
    pub fn levels(&self) -> String {
        let mut text = self.level_range();
        let rank = match (self.is_boss(), self.is_rare(), self.is_elite()) {
            (true, ..) => "boss",
            (_, true, true) => "rare élite",
            (_, true, false) => "rare",
            (_, false, true) => "élite",
            _ => "",
        };
        if !rank.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(rank);
        }
        text
    }

    /// "Dungeon", or "Zone (x, y)".
    pub fn place(&self) -> String {
        if let Some(dungeon) = &self.dungeon {
            return dungeon.clone();
        }
        let mut text = self.zone.clone().unwrap_or_default();
        if let (Some(x), Some(y)) = (self.x, self.y) {
            text += &format!(" ({x:.1}, {y:.1})");
        }
        text
    }
}

/// "12 %", "5,6 %", "0,18 %": two significant digits, French decimal comma.
pub fn format_percent(p: f64) -> String {
    let text = if p >= 10.0 {
        format!("{p:.0}")
    } else if p >= 1.0 {
        format!("{p:.1}")
    } else {
        format!("{p:.2}")
    };
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    format!("{}\u{a0}%", text.replace('.', ","))
}

mod records;
mod sort;
mod zone;
pub use records::{BaseRow, DropName, DropSource, DropsExport, ObservedLoot, ObservedRow};
pub use sort::{DropColumn, compare};
pub use zone::{
    NpcDrop, Zone, ZoneDropRow, ZoneLoot, ZoneNpc, ZoneNpcLevel, ZoneNpcRow, ZoneObservedRow, zones,
};

#[cfg(test)]
mod tests;

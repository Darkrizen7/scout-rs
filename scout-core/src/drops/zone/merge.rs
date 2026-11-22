//! Several zones at once, and the value of a monster for its level.

use super::{ZoneLoot, ZoneNpc};

/// By level (unknown last), then name.
pub(super) fn sort_by_level(npcs: &mut [ZoneNpc]) {
    npcs.sort_by(|a, b| {
        level(a)
            .unwrap_or(f64::MAX)
            .total_cmp(&level(b).unwrap_or(f64::MAX))
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
    });
}

/// Middle of the monster's level range.
fn level(n: &ZoneNpc) -> Option<f64> {
    let min = n.npc.min_level.or(n.npc.max_level)?;
    Some((min + n.npc.max_level.unwrap_or(min)) as f64 / 2.0)
}

impl ZoneNpc {
    /// Value per kill divided by the monster's level: what a kill brings for how hard the monster is.
    pub fn value_per_level(&self, price: impl Fn(i64) -> Option<f64>) -> Option<f64> {
        let level = level(self).filter(|&l| l > 0.0)?;
        Some(self.value_per_kill(price)? / level)
    }
}

impl ZoneLoot {
    /// The monsters of several zones together (a monster listed in two of them once), by level then name;
    /// `zone` joins their names.
    pub fn merge<'a>(parts: impl IntoIterator<Item = &'a ZoneLoot>) -> ZoneLoot {
        let mut names = Vec::new();
        let mut npcs: Vec<ZoneNpc> = Vec::new();
        for part in parts {
            names.push(part.zone.clone());
            for npc in &part.npcs {
                if !npcs.iter().any(|n| n.npc.npc_id == npc.npc.npc_id) {
                    npcs.push(npc.clone());
                }
            }
        }
        sort_by_level(&mut npcs);
        ZoneLoot {
            zone: names.join(" + "),
            npcs,
        }
    }
}

//! Game modes (realm types) of the houses of a region: which modes exist, where to switch when one
//! is chosen, which houses the picker offers.

use super::{ServerChoice, ServerHouse, realm_type};

/// Every realm type, in display order.
const MODES: [&str; 4] = ["Normal", "PvP", "RP", "RP-PvP"];

impl ServerChoice {
    /// The modes present among the houses, in the order Normal, PvP, RP, RP-PvP.
    pub fn modes(&self) -> Vec<&'static str> {
        MODES
            .into_iter()
            .filter(|mode| self.houses.iter().any(|h| realm_type(h.id) == Some(mode)))
            .collect()
    }

    /// The house to switch to when `mode` is chosen: same side as the current house if it exists
    /// in that mode, else the first house of the mode.
    pub fn house_for_mode(&self, current: Option<u32>, mode: &str) -> Option<u32> {
        let in_mode: Vec<&ServerHouse> = self
            .houses
            .iter()
            .filter(|h| realm_type(h.id) == Some(mode))
            .collect();
        let side = current
            .and_then(|id| self.houses.iter().find(|h| h.id == id))
            .map(ServerHouse::side);
        let same_side = side.and_then(|side| in_mode.iter().find(|h| h.side() == side));
        same_side.or(in_mode.first()).map(|h| h.id)
    }

    /// The houses the picker offers: those of the current house's mode; all of them when no house
    /// is chosen or its type is unknown.
    pub fn houses_to_offer(&self, current: Option<u32>) -> Vec<&ServerHouse> {
        let mode = current.and_then(realm_type);
        self.houses
            .iter()
            .filter(|h| mode.is_none() || realm_type(h.id) == mode)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(ids: &[u32]) -> ServerChoice {
        let names = ["Alliance", "Horde", "Flots noirs"];
        ServerChoice {
            regions: vec![],
            region: "eu".into(),
            houses: ids
                .iter()
                .map(|&id| ServerHouse {
                    id,
                    name: names[id as usize % 3].into(),
                    last_update: None,
                })
                .collect(),
        }
    }

    fn ids(houses: Vec<&ServerHouse>) -> Vec<u32> {
        houses.into_iter().map(|h| h.id).collect()
    }

    #[test]
    fn modes_follow_the_fixed_order() {
        let all = choice(&[125, 112, 110, 114, 120]);
        assert_eq!(all.modes(), ["Normal", "PvP", "RP", "RP-PvP"]);
        assert_eq!(choice(&[110, 111, 115]).modes(), ["Normal", "PvP"]);
        assert!(choice(&[112]).modes().is_empty());
    }

    #[test]
    fn switching_mode_keeps_the_side_when_possible() {
        let all = choice(&[110, 111, 112, 113, 114, 119, 120, 121, 123, 124, 125]);
        // Horde Normal (114) -> Horde PvP (113).
        assert_eq!(all.house_for_mode(Some(114), "PvP"), Some(113));
        assert_eq!(all.house_for_mode(Some(114), "RP-PvP"), Some(124));
        // No current house: first of the mode.
        assert_eq!(all.house_for_mode(None, "RP"), Some(119));
        // Same side missing in the mode: first of the mode.
        let few = choice(&[114, 123]);
        assert_eq!(few.house_for_mode(Some(114), "RP-PvP"), Some(123));
        assert_eq!(few.house_for_mode(Some(114), "RP"), None);
    }

    #[test]
    fn picker_offers_the_houses_of_the_current_mode() {
        let all = choice(&[110, 111, 112, 114, 115, 120]);
        assert_eq!(ids(all.houses_to_offer(Some(114))), [111, 114]);
        assert_eq!(ids(all.houses_to_offer(Some(120))), [120]);
        assert_eq!(all.houses_to_offer(None).len(), 6);
        assert_eq!(all.houses_to_offer(Some(112)).len(), 6);
        assert_eq!(all.houses_to_offer(Some(999)).len(), 6);
    }
}

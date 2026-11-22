//! The item list the table shows: filtered, searched and sorted ids, rebuilt only when something changed.

use scout_core::gamedata::{GameData, GameItem};

use super::{Items, Sort};
use crate::state::State;

impl Items {
    pub(super) fn rebuild(&mut self, game: &GameData, state: &State) {
        let key = (
            game as *const GameData as usize,
            state.revision,
            self.filter.clone(),
            self.search.clone(),
            self.sort,
            self.descending,
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        let needle = self.search.trim().to_lowercase();
        let f = &self.filter;
        let mut rows: Vec<&GameItem> = game
            .items
            .values()
            .filter(|i| !f.favorites || state.is_favorite(i.id))
            .filter(|i| f.class_id.is_none() || i.class_id == f.class_id)
            .filter(|i| f.subclass_id.is_none() || i.subclass_id == f.subclass_id)
            .filter(|i| {
                needle.is_empty()
                    || i.id.to_string() == needle
                    || i.name.to_lowercase().contains(&needle)
            })
            .collect();
        match self.sort {
            Sort::Name => rows.sort_by_key(|i| (i.name.to_lowercase(), i.id)),
            Sort::Level => rows.sort_by_key(|i| (i.item_level, i.name.to_lowercase())),
            // Items never seen at the auction house last.
            Sort::Auction => rows.sort_by_key(|i| {
                (
                    state.auction_price(i.id).is_none(),
                    state.auction_price(i.id),
                    i.id,
                )
            }),
        }
        if self.descending {
            rows.reverse();
        }
        self.rows = rows.into_iter().map(|i| i.id).collect();
        self.built_for = Some(key);
    }
}

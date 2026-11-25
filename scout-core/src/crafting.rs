//! What a reagent costs when crafted instead of bought, by the same profession.

use crate::gamedata::{GameData, Recipe};
use crate::pricing;

/// Cheapest way to craft one unit of an item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Craft {
    /// Copper per unit, its own reagents bought or crafted, whichever is cheaper.
    pub unit: f64,
    /// Recipe used (index in `game.recipes`).
    pub recipe: usize,
}

/// Cheapest cost of one unit of `item_id` crafted by `profession`, its reagents bought (`buy`: copper
/// per unit) or crafted themselves when cheaper. None when no recipe of that profession makes it, or
/// when a reagent of every such recipe has no known price.
pub fn craft_cost(
    game: &GameData,
    item_id: i64,
    profession: i64,
    buy: &impl Fn(i64) -> Option<f64>,
) -> Option<Craft> {
    cost(game, item_id, profession, buy, &mut Vec::new())
}

fn recipes(game: &GameData, item_id: i64, profession: i64) -> impl Iterator<Item = usize> + '_ {
    game.made_by
        .get(&item_id)
        .into_iter()
        .flatten()
        .copied()
        .filter(move |&i| game.recipes[i].profession == profession)
}

/// `path`: items being crafted above this one, so a recipe cycle (A from B, B from A) stops.
fn cost(
    game: &GameData,
    item_id: i64,
    profession: i64,
    buy: &impl Fn(i64) -> Option<f64>,
    path: &mut Vec<i64>,
) -> Option<Craft> {
    if path.contains(&item_id) {
        return None;
    }
    path.push(item_id);
    let best = recipes(game, item_id, profession)
        .filter_map(|index| {
            let recipe = &game.recipes[index];
            let made = recipe.creates?.count.max(1);
            let mut total = 0.0;
            for reagent in &recipe.reagents {
                let crafted = cost(game, reagent.item_id, profession, buy, path).map(|c| c.unit);
                let unit = match (buy(reagent.item_id), crafted) {
                    (Some(b), Some(c)) => b.min(c),
                    (b, c) => b.or(c)?,
                };
                total += unit * reagent.count as f64;
            }
            Some(Craft {
                unit: total / made as f64,
                recipe: index,
            })
        })
        .min_by(|a, b| a.unit.total_cmp(&b.unit));
    path.pop();
    best
}

/// What one craft sells for, in copper, through each channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sale {
    /// Auction house price × count, after the auction cut.
    pub auction: Option<i64>,
    /// Merchant buy-back price × count (None when the item cannot be sold to a merchant).
    pub merchant: Option<i64>,
}

impl Sale {
    /// Best of the two channels.
    pub fn best(self) -> Option<i64> {
        self.auction.max(self.merchant)
    }
}

/// Sale value of what a recipe makes; None for a recipe without a product.
/// `auction`: copper per unit at the auction house; `merchant`: merchant buy-back per unit.
pub fn sale(
    recipe: &Recipe,
    auction: &impl Fn(i64) -> Option<i64>,
    merchant: &impl Fn(i64) -> Option<i64>,
) -> Option<Sale> {
    let made = recipe.creates?;
    Some(Sale {
        auction: auction(made.item_id).map(|unit| {
            (unit as f64 * made.count as f64 * (1.0 - pricing::AUCTION_CUT)).round() as i64
        }),
        merchant: merchant(made.item_id)
            .map(|unit| unit * made.count)
            .filter(|&p| p > 0),
    })
}

/// Profit of one craft: best sale minus the cost of every reagent (bought, or crafted by the same
/// profession when cheaper). None when it cannot be told: no product, no sale price, or a reagent
/// without a known price.
pub fn profit(
    game: &GameData,
    recipe: &Recipe,
    auction: &impl Fn(i64) -> Option<i64>,
    merchant: &impl Fn(i64) -> Option<i64>,
    buy: &impl Fn(i64) -> Option<f64>,
) -> Option<i64> {
    let best = sale(recipe, auction, merchant)?.best()?;
    let mut cost = 0.0;
    for reagent in &recipe.reagents {
        let crafted = craft_cost(game, reagent.item_id, recipe.profession, buy).map(|c| c.unit);
        let unit = match (buy(reagent.item_id), crafted) {
            (Some(b), Some(c)) => b.min(c),
            (b, c) => b.or(c)?,
        };
        cost += unit * reagent.count as f64;
    }
    Some(best - cost.round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::{Reagent, Recipe, Tables};

    fn recipe(profession: i64, creates: (i64, i64), reagents: &[(i64, i64)]) -> Recipe {
        let reagent = |(item_id, count)| Reagent { item_id, count };
        Recipe {
            spell_id: 0,
            name: String::new(),
            profession,
            required: None,
            yellow: 0,
            grey: 0,
            creates: Some(reagent(creates)),
            reagents: reagents.iter().copied().map(reagent).collect(),
            taught_by: vec![],
        }
    }

    fn game(recipes: Vec<Recipe>) -> GameData {
        GameData::new(
            String::new(),
            0,
            Tables {
                recipes,
                ..Tables::default()
            },
        )
    }

    const LEATHER: i64 = 165;

    #[test]
    fn crafts_with_bought_or_crafted_reagents() {
        // 2 = cured hide (1 hide + 1 salt, makes 1); 3 = leather (2 hides, makes 2).
        let game = game(vec![
            recipe(LEATHER, (2, 1), &[(1, 1), (9, 1)]),
            recipe(LEATHER, (3, 2), &[(1, 2)]),
            recipe(LEATHER, (4, 1), &[(2, 2), (3, 1)]),
            recipe(171, (5, 1), &[(1, 1)]),
        ]);
        let buy = |id| match id {
            1 => Some(10.0),
            9 => Some(5.0),
            3 => Some(100.0),
            _ => None,
        };
        let craft = |id, prof| craft_cost(&game, id, prof, &buy);
        assert_eq!(
            craft(2, LEATHER),
            Some(Craft {
                unit: 15.0,
                recipe: 0
            })
        );
        assert_eq!(craft(3, LEATHER).map(|c| c.unit), Some(10.0));
        // Cured hides crafted (2 × 15) and leather crafted (10, cheaper than its 100 at the AH).
        assert_eq!(craft(4, LEATHER).map(|c| c.unit), Some(40.0));
        // Another profession's recipe does not count.
        assert_eq!(craft(5, LEATHER), None);
        assert_eq!(craft(5, 171).map(|c| c.unit), Some(10.0));
    }

    #[test]
    fn unknown_reagent_or_cycle() {
        let game = game(vec![
            recipe(LEATHER, (2, 1), &[(7, 1)]),
            recipe(LEATHER, (3, 1), &[(4, 1)]),
            recipe(LEATHER, (4, 1), &[(3, 1)]),
        ]);
        let buy = |id| (id == 4).then_some(8.0);
        assert_eq!(craft_cost(&game, 2, LEATHER, &buy), None);
        assert_eq!(
            craft_cost(&game, 3, LEATHER, &buy).map(|c| c.unit),
            Some(8.0)
        );
        assert_eq!(craft_cost(&game, 4, LEATHER, &|_| None), None);
    }

    #[test]
    fn profit_cases() {
        let game = game(vec![recipe(LEATHER, (2, 2), &[(1, 3)])]);
        let r = &game.recipes[0];
        let buy = |id| (id == 1).then_some(10.0);
        let merchant = |_| None;
        // 2 items at 100 each, minus 5 %, minus 30 for the reagents.
        let rich = |id| (id == 2).then_some(100);
        assert_eq!(profit(&game, r, &rich, &merchant, &buy), Some(190 - 30));
        // Sells for less than the reagents cost.
        let poor = |id| (id == 2).then_some(5);
        assert_eq!(profit(&game, r, &poor, &merchant, &buy), Some(10 - 30));
        // Merchant buy-back wins over the auction house.
        let vendor = |id| (id == 2).then_some(50);
        assert_eq!(profit(&game, r, &poor, &vendor, &buy), Some(100 - 30));
        // No price for the product, or for a reagent.
        assert_eq!(profit(&game, r, &|_| None, &merchant, &buy), None);
        assert_eq!(profit(&game, r, &rich, &merchant, &|_| None), None);
        // No product.
        let mut none = r.clone();
        none.creates = None;
        assert_eq!(profit(&game, &none, &rich, &merchant, &buy), None);
    }
}

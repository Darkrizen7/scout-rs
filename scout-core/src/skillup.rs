//! Cheapest recipes to level a profession: what one skill point costs, by recipe.

use crate::crafting;
use crate::gamedata::{GameData, Recipe};

/// Difficulty of a recipe for a given skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillColor {
    Orange,
    Yellow,
    Green,
    Grey,
}

/// Color of `recipe` for a character with `skill` points.
pub fn color(recipe: &Recipe, skill: i64) -> SkillColor {
    if skill < recipe.yellow {
        SkillColor::Orange
    } else if skill < recipe.green() {
        SkillColor::Yellow
    } else if skill < recipe.grey {
        SkillColor::Green
    } else {
        SkillColor::Grey
    }
}

/// Chance (0..=1) that a craft gives a skill point (Classic rule).
pub fn chance(recipe: &Recipe, skill: i64) -> f64 {
    match color(recipe, skill) {
        SkillColor::Orange => 1.0,
        SkillColor::Grey => 0.0,
        _ => {
            let span = (recipe.grey - recipe.yellow).max(1) as f64;
            ((recipe.grey - skill) as f64 / span).clamp(0.0, 1.0)
        }
    }
}

/// Whether the character can craft the recipe and still gain a point from it.
pub fn learnable(recipe: &Recipe, skill: i64) -> bool {
    recipe.required.unwrap_or(1) <= skill && skill < recipe.grey
}

/// Expected cost in copper of one skill point; None when the craft never gives one.
/// `net_cost`: copper spent per craft (negative: the craft earns money).
pub fn cost_per_point(net_cost: i64, chance: f64) -> Option<i64> {
    (chance > 0.0).then(|| (net_cost as f64 / chance).round() as i64)
}

/// A recipe that can still give a skill point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUpRow {
    /// Index in `game.recipes`.
    pub recipe: usize,
    pub color: SkillColor,
    pub chance: f64,
    /// Copper spent per craft (negative: it earns money); None when a price is unknown.
    pub net_cost: Option<i64>,
    /// Expected copper per skill point.
    pub per_point: Option<i64>,
}

/// Recipes (indexes in `game.recipes`) of the character's profession that give a point at `skill`,
/// cheapest point first; unknown prices last, then by name.
pub fn rank(
    game: &GameData,
    recipes: impl IntoIterator<Item = usize>,
    skill: i64,
    auction: &impl Fn(i64) -> Option<i64>,
    merchant: &impl Fn(i64) -> Option<i64>,
    buy: &impl Fn(i64) -> Option<f64>,
) -> Vec<SkillUpRow> {
    let mut rows: Vec<SkillUpRow> = recipes
        .into_iter()
        .filter(|&i| learnable(&game.recipes[i], skill))
        .map(|i| {
            let r = &game.recipes[i];
            let chance = chance(r, skill);
            let net_cost = crafting::profit(game, r, auction, merchant, buy).map(|p| -p);
            SkillUpRow {
                recipe: i,
                color: color(r, skill),
                chance,
                net_cost,
                per_point: net_cost.and_then(|c| cost_per_point(c, chance)),
            }
        })
        .collect();
    rows.sort_by_cached_key(|row| {
        (
            row.per_point.is_none(),
            row.per_point,
            game.recipes[row.recipe].name.to_lowercase(),
        )
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::{Reagent, Tables};

    fn recipe(name: &str, required: Option<i64>, yellow: i64, grey: i64, item: i64) -> Recipe {
        Recipe {
            spell_id: 0,
            name: name.to_owned(),
            profession: 1,
            required,
            yellow,
            grey,
            creates: Some(Reagent {
                item_id: item,
                count: 1,
            }),
            reagents: vec![Reagent {
                item_id: 100,
                count: 1,
            }],
            taught_by: vec![],
        }
    }

    // yellow 50, green 75, grey 100.
    fn sample() -> Recipe {
        recipe("a", Some(25), 50, 100, 1)
    }

    #[test]
    fn colors_at_the_boundaries() {
        let r = sample();
        assert_eq!(color(&r, 49), SkillColor::Orange);
        assert_eq!(color(&r, 50), SkillColor::Yellow);
        assert_eq!(color(&r, 74), SkillColor::Yellow);
        assert_eq!(color(&r, 75), SkillColor::Green);
        assert_eq!(color(&r, 99), SkillColor::Green);
        assert_eq!(color(&r, 100), SkillColor::Grey);
    }

    #[test]
    fn chance_by_color() {
        let r = sample();
        assert_eq!(chance(&r, 10), 1.0);
        assert_eq!(chance(&r, 75), 0.5);
        assert_eq!(chance(&r, 100), 0.0);
        let flat = recipe("b", None, 50, 50, 1);
        assert_eq!(chance(&flat, 50), 0.0);
    }

    #[test]
    fn learnable_needs_required_skill_and_a_point_left() {
        let r = sample();
        assert!(!learnable(&r, 24));
        assert!(learnable(&r, 25));
        assert!(!learnable(&r, 100));
        assert!(learnable(&recipe("c", None, 5, 20, 1), 1));
    }

    #[test]
    fn a_craft_that_earns_money_is_a_negative_cost() {
        assert_eq!(cost_per_point(100, 0.5), Some(200));
        assert_eq!(cost_per_point(-100, 0.5), Some(-200));
        assert_eq!(cost_per_point(100, 0.0), None);
    }

    #[test]
    fn ranks_cheapest_point_first_unknown_last() {
        let game = GameData::new(
            String::new(),
            0,
            Tables {
                recipes: vec![
                    recipe("cher", Some(1), 50, 100, 1),
                    recipe("rentable", Some(1), 50, 100, 2),
                    recipe("inconnu", Some(1), 50, 100, 3),
                    recipe("gris", Some(1), 5, 10, 1),
                ],
                ..Tables::default()
            },
        );
        let auction = |id: i64| match id {
            1 => Some(0),
            2 => Some(1000),
            _ => None,
        };
        let rows = rank(&game, 0..4, 10, &auction, &|_| None, &|_| Some(300.0));
        let order: Vec<usize> = rows.iter().map(|r| r.recipe).collect();
        assert_eq!(order, vec![1, 0, 2]);
        assert!(rows[0].per_point.unwrap() < 0);
        assert_eq!(rows[2].per_point, None);
    }
}

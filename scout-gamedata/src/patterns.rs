//! Skill levels of recipe items (patterns, plans, formulas…) as the game shows them in their
//! tooltip: a few differ from `RequiredSkillRank` in the client tables (server-side fixes).

use std::collections::{BTreeMap, HashMap};

use scout_core::gamedata::Tables;

/// Parses `item_id,skill` lines (a header line is skipped, as are lines that do not parse).
pub fn parse_pattern_skills(csv: &str) -> HashMap<i64, i64> {
    csv.lines()
        .filter_map(|line| {
            let (item, skill) = line.split_once(',')?;
            Some((item.trim().parse().ok()?, skill.trim().parse().ok()?))
        })
        .collect()
}

/// Sets the skill to learn each recipe from the levels seen in game for its recipe items (the
/// highest, as the assembly does). Returns whether a recipe changed.
pub fn apply_pattern_skills(tables: &mut Tables, skills: &HashMap<i64, i64>) -> bool {
    let mut changed = false;
    for recipe in &mut tables.recipes {
        let seen = recipe
            .taught_by
            .iter()
            .filter_map(|item| skills.get(item))
            .copied()
            .max();
        if seen.is_some() && seen != recipe.required {
            recipe.required = seen;
            changed = true;
        }
    }
    changed
}

/// What a merge of seen levels into the known ones changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub added: usize,
    /// `(item, old level, new level)`.
    pub changed: Vec<(i64, i64, i64)>,
    pub unchanged: usize,
}

/// Merges the levels seen in game into the known ones: the file only grows (items missing from
/// the seen list stay) and a seen level replaces the old one.
pub fn merge_pattern_skills(
    existing: &HashMap<i64, i64>,
    seen: &[(i64, i64)],
) -> (BTreeMap<i64, i64>, Summary) {
    let mut merged: BTreeMap<i64, i64> = existing.iter().map(|(&i, &s)| (i, s)).collect();
    let mut summary = Summary::default();
    for &(item, skill) in seen {
        match merged.insert(item, skill) {
            None => summary.added += 1,
            Some(old) if old != skill => summary.changed.push((item, old, skill)),
            Some(_) => summary.unchanged += 1,
        }
    }
    summary.changed.sort_unstable();
    (merged, summary)
}

/// Writes `item_id,skill` lines (header first, sorted by item, final newline).
pub fn write_pattern_skills(skills: &BTreeMap<i64, i64>) -> String {
    let mut out = String::from("item_id,skill\n");
    for (item, skill) in skills {
        out.push_str(&format!("{item},{skill}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use scout_core::gamedata::Recipe;

    fn recipe(required: Option<i64>, taught_by: Vec<i64>) -> Recipe {
        Recipe {
            spell_id: 1,
            name: String::new(),
            profession: 164,
            required,
            yellow: 0,
            grey: 0,
            creates: None,
            reagents: vec![],
            taught_by,
        }
    }

    #[test]
    fn header_and_bad_lines_are_skipped() {
        let skills = parse_pattern_skills("item_id,skill\n6044,150\noops\n7976, 215\n");
        assert_eq!(skills, HashMap::from([(6044, 150), (7976, 215)]));
    }

    #[test]
    fn levels_seen_in_game_win() {
        let mut tables = Tables {
            recipes: vec![
                recipe(Some(125), vec![6044]),
                recipe(None, vec![3, 4]),
                recipe(Some(35), vec![5083]),
                recipe(Some(1), vec![9]),
            ],
            ..Tables::default()
        };
        let skills = HashMap::from([(6044, 150), (3, 200), (4, 210), (5083, 35)]);
        assert!(apply_pattern_skills(&mut tables, &skills));
        let required: Vec<_> = tables.recipes.iter().map(|r| r.required).collect();
        assert_eq!(required, [Some(150), Some(210), Some(35), Some(1)]);
        assert!(
            !apply_pattern_skills(&mut tables, &skills),
            "nothing left to fix"
        );
    }

    #[test]
    fn merge_adds_changes_and_keeps_absent() {
        let existing = HashMap::from([(1, 10), (2, 20), (3, 30)]);
        let (merged, summary) = merge_pattern_skills(&existing, &[(2, 20), (3, 35), (4, 40)]);
        assert_eq!(merged, BTreeMap::from([(1, 10), (2, 20), (3, 35), (4, 40)]));
        assert_eq!(summary.added, 1);
        assert_eq!(summary.changed, vec![(3, 30, 35)]);
        assert_eq!(summary.unchanged, 1);
    }

    #[test]
    fn written_file_is_sorted_and_parses_back() {
        let skills = BTreeMap::from([(7976, 215), (6044, 150)]);
        let csv = write_pattern_skills(&skills);
        assert_eq!(csv, "item_id,skill\n6044,150\n7976,215\n");
        assert_eq!(
            parse_pattern_skills(&csv),
            HashMap::from([(6044, 150), (7976, 215)])
        );
    }
}

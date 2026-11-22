//! Joins the client tables into items, professions and recipes (same logic as alcaras/forever-ref).

use std::collections::{BTreeMap, HashMap};

mod maps;

use anyhow::{Context, Result};
use csv::StringRecord;

use scout_core::gamedata::{Category, GameItem, Profession, Reagent, Recipe, Tables};

/// Profession skill lines: primary, secondary (cooking, first aid, fishing), poisons.
const PROFESSIONS: [i64; 13] = [
    171, 164, 333, 202, 182, 165, 186, 393, 197, 185, 129, 356, 40,
];
/// SpellEffect: create an item.
const CREATE_ITEM: i64 = 24;
/// ItemEffect trigger: learn the spell (recipe items).
const LEARN: i64 = 6;

/// A CSV table with columns looked up by name.
struct Table {
    name: &'static str,
    columns: HashMap<String, usize>,
    rows: Vec<StringRecord>,
}

impl Table {
    fn parse(name: &'static str, csv: &[u8]) -> Result<Self> {
        let mut reader = csv::Reader::from_reader(csv);
        let columns = reader
            .headers()
            .with_context(|| format!("en-têtes de {name}"))?
            .iter()
            .enumerate()
            .map(|(i, h)| (h.to_owned(), i))
            .collect();
        let rows = reader
            .records()
            .collect::<Result<_, _>>()
            .with_context(|| format!("lignes de {name}"))?;
        Ok(Self {
            name,
            columns,
            rows,
        })
    }

    /// Column index, checked once so a renamed column fails loudly instead of reading zeros.
    fn column(&self, column: &str) -> Result<usize> {
        self.columns
            .get(column)
            .copied()
            .with_context(|| format!("colonne {column} absente de {}", self.name))
    }
}

fn int(row: &StringRecord, column: usize) -> i64 {
    let text = row.get(column).unwrap_or("");
    text.parse()
        .unwrap_or_else(|_| text.parse::<f64>().map_or(0, |f| f.round() as i64))
}

fn text(row: &StringRecord, column: usize) -> String {
    row.get(column).unwrap_or("").to_owned()
}

/// `tables`: table name → CSV, for every name in `download::TABLES`.
pub fn assemble(tables: &HashMap<&str, Vec<u8>>) -> Result<Tables> {
    let table = |name: &'static str| -> Result<Table> {
        Table::parse(
            name,
            tables
                .get(name)
                .with_context(|| format!("table {name} manquante"))?,
        )
    };

    // Items: names and levels from ItemSparse, category from Item.
    let item = table("Item")?;
    let (id, class, subclass) = (
        item.column("ID")?,
        item.column("ClassID")?,
        item.column("SubclassID")?,
    );
    let categories: HashMap<i64, (i64, i64)> = item
        .rows
        .iter()
        .map(|r| (int(r, id), (int(r, class), int(r, subclass))))
        .collect();
    let sparse = table("ItemSparse")?;
    let [id, name, quality, level, req_level, req_skill, req_rank] = [
        "ID",
        "Display_lang",
        "OverallQualityID",
        "ItemLevel",
        "RequiredLevel",
        "RequiredSkill",
        "RequiredSkillRank",
    ]
    .map(|c| sparse.column(c));
    let [id, name, quality, level, req_level, req_skill, req_rank] = [
        id?, name?, quality?, level?, req_level?, req_skill?, req_rank?,
    ];
    let [buy, stack, sell] =
        ["BuyPrice", "VendorStackCount", "SellPrice"].map(|c| sparse.column(c));
    let [buy, stack, sell] = [buy?, stack?, sell?];
    let mut items = Vec::with_capacity(sparse.rows.len());
    let mut item_skill: HashMap<i64, (i64, i64)> = HashMap::new();
    for r in &sparse.rows {
        let item_id = int(r, id);
        let category = categories.get(&item_id);
        items.push(GameItem {
            id: item_id,
            name: text(r, name),
            quality: int(r, quality),
            item_level: int(r, level),
            required_level: int(r, req_level),
            class_id: category.map(|c| c.0),
            subclass_id: category.map(|c| c.1),
            buy_price: int(r, buy),
            vendor_stack: int(r, stack),
            sell_price: int(r, sell),
        });
        item_skill.insert(item_id, (int(r, req_skill), int(r, req_rank)));
    }

    // Professions, and the child skill lines Forever moved recipes to (2937 → 171…).
    let lines = table("SkillLine")?;
    let (id, name, parent) = (
        lines.column("ID")?,
        lines.column("DisplayName_lang")?,
        lines.column("ParentSkillLineID")?,
    );
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    let mut names: HashMap<i64, String> = HashMap::new();
    for r in &lines.rows {
        names.insert(int(r, id), text(r, name));
        if int(r, parent) != 0 {
            parent_of.insert(int(r, id), int(r, parent));
        }
    }
    let profession_of = |line: i64| -> Option<i64> {
        let line = parent_of.get(&line).copied().unwrap_or(line);
        PROFESSIONS.contains(&line).then_some(line)
    };
    let professions = PROFESSIONS
        .iter()
        .filter_map(|&p| {
            Some(Profession {
                id: p,
                name: names.get(&p)?.clone(),
            })
        })
        .collect();

    // Spells: names, created item, reagents.
    let spell_names = table("SpellName")?;
    let (id, name) = (spell_names.column("ID")?, spell_names.column("Name_lang")?);
    let spell_name: HashMap<i64, String> = spell_names
        .rows
        .iter()
        .map(|r| (int(r, id), text(r, name)))
        .collect();

    let effects = table("SpellEffect")?;
    let (spell, effect, made, points) = (
        effects.column("SpellID")?,
        effects.column("Effect")?,
        effects.column("EffectItemType")?,
        effects.column("EffectBasePointsF")?,
    );
    let mut creates: HashMap<i64, Reagent> = HashMap::new();
    for r in &effects.rows {
        if int(r, effect) == CREATE_ITEM && int(r, made) != 0 {
            creates.entry(int(r, spell)).or_insert(Reagent {
                item_id: int(r, made),
                count: int(r, points).max(1),
            });
        }
    }

    let reagent_table = table("SpellReagents")?;
    let spell = reagent_table.column("SpellID")?;
    let slots: Vec<(usize, usize)> = (0..8)
        .map(|i| {
            Ok((
                reagent_table.column(&format!("Reagent_{i}"))?,
                reagent_table.column(&format!("ReagentCount_{i}"))?,
            ))
        })
        .collect::<Result<_>>()?;
    let mut reagents: HashMap<i64, Vec<Reagent>> = HashMap::new();
    for r in &reagent_table.rows {
        let list: Vec<Reagent> = slots
            .iter()
            .filter(|&&(item, _)| int(r, item) > 0)
            .map(|&(item, count)| Reagent {
                item_id: int(r, item),
                count: int(r, count).max(1),
            })
            .collect();
        reagents.entry(int(r, spell)).or_insert(list);
    }

    // Recipe items: item effect "learn" → taught spell.
    let item_effects = table("ItemEffect")?;
    let (id, trigger, spell) = (
        item_effects.column("ID")?,
        item_effects.column("TriggerType")?,
        item_effects.column("SpellID")?,
    );
    let learns: HashMap<i64, i64> = item_effects
        .rows
        .iter()
        .filter(|r| int(r, trigger) == LEARN)
        .map(|r| (int(r, id), int(r, spell)))
        .collect();
    let links = table("ItemXItemEffect")?;
    let (effect_id, item_id) = (links.column("ItemEffectID")?, links.column("ItemID")?);
    let mut taught_by: HashMap<i64, Vec<i64>> = HashMap::new();
    for r in &links.rows {
        if let Some(&spell) = learns.get(&int(r, effect_id)) {
            taught_by.entry(spell).or_default().push(int(r, item_id));
        }
    }

    // Recipes: profession abilities that make something or use reagents (one per spell).
    let abilities = table("SkillLineAbility")?;
    let [line, spell, min, low, high] = [
        "SkillLine",
        "Spell",
        "MinSkillLineRank",
        "TrivialSkillLineRankLow",
        "TrivialSkillLineRankHigh",
    ]
    .map(|c| abilities.column(c));
    let [line, spell, min, low, high] = [line?, spell?, min?, low?, high?];
    let mut recipes: BTreeMap<i64, Recipe> = BTreeMap::new();
    for r in &abilities.rows {
        let spell_id = int(r, spell);
        let (Some(profession), Some(name)) =
            (profession_of(int(r, line)), spell_name.get(&spell_id))
        else {
            continue;
        };
        let reagents = reagents.get(&spell_id).cloned().unwrap_or_default();
        let creates = creates.get(&spell_id).copied();
        if name.is_empty()
            || (reagents.is_empty() && creates.is_none())
            || recipes.contains_key(&spell_id)
        {
            continue;
        }
        let mut taught: Vec<i64> = taught_by.get(&spell_id).cloned().unwrap_or_default();
        taught.sort_unstable();
        taught.dedup();
        // The skill to learn it is carried by its recipe item; trainer recipes only by a minimum above 1.
        let from_items = taught
            .iter()
            .filter_map(|i| item_skill.get(i))
            .filter(|(skill, _)| profession_of(*skill) == Some(profession))
            .map(|&(_, rank)| rank)
            .max();
        let required = from_items.or_else(|| Some(int(r, min)).filter(|&m| m > 1));
        recipes.insert(
            spell_id,
            Recipe {
                spell_id,
                name: name.clone(),
                profession,
                required,
                yellow: int(r, low),
                grey: int(r, high),
                creates,
                reagents,
                taught_by: taught,
            },
        );
    }
    // Item categories: classes, then subclasses with their detailed name ("Epées à deux mains").
    let classes = table("ItemClass")?;
    let (class, name) = (
        classes.column("ClassID")?,
        classes.column("ClassName_lang")?,
    );
    let mut categories: Vec<Category> = classes
        .rows
        .iter()
        .map(|r| Category {
            class_id: int(r, class),
            subclass_id: None,
            name: text(r, name),
        })
        .collect();
    let subclasses = table("ItemSubClass")?;
    let [class, sub, short, verbose] = [
        "ClassID",
        "SubClassID",
        "DisplayName_lang",
        "VerboseName_lang",
    ]
    .map(|c| subclasses.column(c));
    let [class, sub, short, verbose] = [class?, sub?, short?, verbose?];
    categories.extend(
        subclasses
            .rows
            .iter()
            .filter(|r| int(r, sub) >= 0)
            .map(|r| {
                let name = Some(text(r, verbose))
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| text(r, short));
                Category {
                    class_id: int(r, class),
                    subclass_id: Some(int(r, sub)),
                    name,
                }
            }),
    );

    let maps = maps::maps(&table("UiMap")?, &table("UiMapAssignment")?)?;

    Ok(Tables {
        items,
        professions,
        recipes: recipes.into_values().collect(),
        categories,
        maps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kingdoms_place(maps: &[scout_core::map::GameMap]) -> scout_core::map::Frac {
        maps[0].place(&scout_core::map::GameMap {
            y_max: 0.0,
            ..maps[1].clone()
        })
    }

    /// A tiny Forever: linen cloth → bolt (tailoring, through a child skill line), and a pattern for boots.
    fn tables() -> HashMap<&'static str, Vec<u8>> {
        let mut t = HashMap::new();
        let mut put = |name, csv: &str| {
            t.insert(name, csv.as_bytes().to_vec());
        };
        put(
            "ItemSparse",
            "ID,Display_lang,OverallQualityID,ItemLevel,RequiredLevel,RequiredSkill,RequiredSkillRank,BuyPrice,VendorStackCount,SellPrice\n\
             2589,Etoffe de lin,1,5,0,0,0,55,1,13\n2996,Rouleau d'étoffe en lin,1,5,0,0,0,160,1,40\n\
             9001,Bottes,2,20,15,0,0,900,1,180\n9002,Patron : bottes,2,20,0,2948,55,20,5,5\n",
        );
        put(
            "Item",
            "ID,ClassID,SubclassID\n2589,7,5\n2996,7,5\n9001,4,1\n9002,9,2\n",
        );
        put(
            "ItemClass",
            "ID,ClassName_lang,ClassID\n1,Artisanat,7\n2,Recette,9\n",
        );
        put(
            "ItemSubClass",
            "DisplayName_lang,VerboseName_lang,ID,ClassID,SubClassID\nNewItem,,1,0,-1\nTissu,,2,7,5\n",
        );
        put("ItemEffect", "ID,TriggerType,SpellID\n1,6,8001\n2,0,999\n");
        put(
            "ItemXItemEffect",
            "ID,ItemEffectID,ItemID\n1,1,9002\n2,2,2996\n",
        );
        put(
            "SpellName",
            "ID,Name_lang\n2963,Rouleau d'étoffe de lin\n8001,Bottes\n7777,Attaque\n",
        );
        put(
            "SpellEffect",
            "ID,SpellID,Effect,EffectItemType,EffectBasePointsF\n1,2963,24,2996,1.0\n2,8001,24,9001,1\n3,7777,2,0,10\n",
        );
        let mut reagents = String::from("ID,SpellID");
        for i in 0..8 {
            reagents += &format!(",Reagent_{i}");
        }
        for i in 0..8 {
            reagents += &format!(",ReagentCount_{i}");
        }
        reagents += "\n1,2963,2589,0,0,0,0,0,0,0,2,0,0,0,0,0,0,0\n2,8001,2996,2589,0,0,0,0,0,0,3,1,0,0,0,0,0,0\n";
        put("SpellReagents", &reagents);
        put(
            "SkillLineAbility",
            "SkillLine,Spell,MinSkillLineRank,TrivialSkillLineRankLow,TrivialSkillLineRankHigh\n\
             2948,2963,1,25,50\n197,2963,1,25,50\n197,8001,1,60,65\n26,7777,1,0,0\n",
        );
        put(
            "SkillLine",
            "ID,DisplayName_lang,ParentSkillLineID\n197,Couture,0\n2948,Couture,197\n26,Armes,0\n",
        );
        put(
            "UiMap",
            "Name_lang,ID,ParentUiMapID,Type\nAzeroth,947,0,1\nRoyaumes de l'est,1415,947,2\n\
             Bois de la Pénombre,1431,1415,3\n",
        );
        put(
            "UiMapAssignment",
            "UiMin_0,UiMin_1,UiMax_0,UiMax_1,Region_0,Region_1,Region_2,Region_3,Region_4,Region_5,UiMapID\n\
             0.55,0.09,0.89,0.86,-16000,-7466,-1000000,6933,8000,1000000,947\n\
             0,0,1,1,-16000,-1000,-1000000,8000,1000,1000000,1415\n\
             0,0,1,1,-11516.5,-1866.75,-1000000,-9716.5,833.25,1000000,1431\n",
        );
        t
    }

    #[test]
    fn builds_recipes_with_reagents_and_patterns() {
        let Tables {
            items,
            professions,
            recipes,
            categories,
            maps,
        } = assemble(&tables()).unwrap();
        // Continent and zone kept, world map left out (it has two partial assignments).
        assert_eq!(maps.len(), 2);
        assert_eq!(maps[1].name, "Bois de la Pénombre");
        assert_eq!((maps[1].x_min, maps[1].y_max), (-11516.5, 833.25));
        assert_eq!(kingdoms_place(&maps).left, 0.5);
        assert_eq!(categories.len(), 3);
        assert_eq!(
            categories[2].name, "Tissu",
            "short name when there is no detailed one"
        );
        assert_eq!(items.len(), 4);
        assert_eq!(
            items.iter().find(|i| i.id == 9001).unwrap().class_id,
            Some(4)
        );
        let pattern = items.iter().find(|i| i.id == 9002).unwrap();
        assert_eq!(
            (pattern.sell_price, pattern.buy_price, pattern.vendor_stack),
            (5, 20, 5)
        );
        assert_eq!(
            professions
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Couture"]
        );
        assert_eq!(recipes.len(), 2, "one per spell, combat spells left out");

        let bolt = &recipes[0];
        assert_eq!(
            (bolt.spell_id, bolt.profession, bolt.yellow, bolt.grey),
            (2963, 197, 25, 50)
        );
        assert_eq!(
            bolt.creates,
            Some(Reagent {
                item_id: 2996,
                count: 1
            })
        );
        assert_eq!(
            bolt.reagents,
            [Reagent {
                item_id: 2589,
                count: 2
            }]
        );
        assert_eq!(bolt.required, None, "trainer recipe at 1");

        let boots = &recipes[1];
        assert_eq!(boots.taught_by, [9002]);
        assert_eq!(
            boots.required,
            Some(55),
            "from the pattern, on a child skill line"
        );
        assert_eq!(boots.reagents.len(), 2);
    }

    #[test]
    fn renamed_column_is_an_error() {
        let mut t = tables();
        t.insert("SpellName", b"ID,Nom\n1,x\n".to_vec());
        assert!(assemble(&t).is_err());
    }
}

use super::*;

fn base(kind: &str, source_id: i64, chance: Option<f64>, kills: Option<i64>) -> BaseRow {
    BaseRow {
        kind: kind.into(),
        source_id,
        chance,
        quest_only: false,
        more: 0,
        name: Some(format!("Source {source_id}")),
        min_level: Some(24),
        max_level: Some(25),
        rank: Some(0),
        zone: Some("Bois de la Pénombre".into()),
        x: Some(25.1),
        y: Some(38.2),
        dungeon: None,
        kills,
        skins: None,
    }
}

fn observed(npc_id: i64, kind: &str, drops: i64, kills: i64) -> ObservedRow {
    ObservedRow {
        npc_id,
        kind: kind.into(),
        drops,
        quantity: drops * 2,
        name: Some("Nouveau monstre".into()),
        zone: Some("Zone Forever".into()),
        kills,
        skins: kills,
    }
}

#[test]
fn merges_reference_and_observed_loot() {
    let rows = vec![
        base("npc", 1, Some(12.0), Some(10)),
        base("npc", 2, Some(40.0), Some(50)),
        base("npc", 0, None, None),
        base("quest", 6, None, None),
    ];
    let mut rows = rows;
    rows[2].more = 77;
    let item = ItemDrops::new(
        rows,
        vec![observed(1, "loot", 3, 10), observed(9, "loot", 5, 25)],
        85,
    );
    assert_eq!(item.more, vec![(DropKind::Npc, 77)]);
    assert_eq!(item.total_kills, 85);
    let npcs: Vec<i64> = item.of(DropKind::Npc).map(|d| d.source_id).collect();
    // Reference chance first (40 %, 12 %), then the monster only seen in game (no reference chance).
    assert_eq!(npcs, vec![2, 1, 9]);
    let new = item.of(DropKind::Npc).find(|d| d.source_id == 9).unwrap();
    assert_eq!(new.name, "Nouveau monstre");
    assert_eq!(new.chance, None);
    assert_eq!(new.npc.as_ref().unwrap().place(), "Zone Forever");
    let two = item.of(DropKind::Npc).find(|d| d.source_id == 2).unwrap();
    assert_eq!(
        two.observed,
        Some(Observed {
            drops: 0,
            total: 50,
            quantity: 0
        })
    );
    assert_eq!(item.drops.last().unwrap().kind, DropKind::Quest);
}

#[test]
fn skinning_uses_skinned_corpses() {
    let mut row = base("skinning", 1, Some(80.0), None);
    row.skins = Some(4);
    let item = ItemDrops::new(vec![row], vec![observed(1, "skin", 3, 4)], 4);
    let o = item.drops[0].observed.unwrap();
    assert_eq!((o.drops, o.total), (3, 4));
    assert_eq!(o.average_quantity(), 2.0);
}

#[test]
fn wilson_interval_narrows_with_kills() {
    let few = Observed {
        drops: 3,
        total: 10,
        quantity: 3,
    };
    let many = Observed {
        drops: 300,
        total: 1000,
        quantity: 300,
    };
    let (a, b) = few.interval();
    let (c, d) = many.interval();
    assert!((a - 10.8).abs() < 0.1 && (b - 60.3).abs() < 0.1, "{a} {b}");
    assert!(c < 30.0 && d > 30.0 && d - c < 6.0);
    let none = Observed {
        drops: 0,
        total: 20,
        quantity: 0,
    };
    assert_eq!(none.interval().0, 0.0);
}

#[test]
fn levels_and_place() {
    let mut npc = DropNpc {
        min_level: Some(24),
        max_level: Some(25),
        zone: Some("Bois de la Pénombre".into()),
        x: Some(25.1),
        y: Some(38.2),
        ..DropNpc::default()
    };
    assert_eq!(npc.levels(), "24-25");
    assert_eq!(npc.place(), "Bois de la Pénombre (25.1, 38.2)");
    npc.max_level = Some(24);
    npc.rank = Some(1);
    npc.dungeon = Some("Les Mortemines".into());
    assert_eq!(npc.levels(), "24 élite");
    assert_eq!(npc.place(), "Les Mortemines");
}

#[test]
fn percents_in_french() {
    assert_eq!(format_percent(12.0), "12\u{a0}%");
    assert_eq!(format_percent(5.6), "5,6\u{a0}%");
    assert_eq!(format_percent(0.18), "0,18\u{a0}%");
    assert_eq!(format_percent(39.0), "39\u{a0}%");
    assert_eq!(format_percent(2.0), "2\u{a0}%");
}

#[test]
fn sorts_by_column_with_unknown_values_last() {
    let item = ItemDrops::new(
        vec![
            base("npc", 1, Some(12.0), Some(10)),
            base("npc", 2, Some(40.0), Some(50)),
            base("npc", 3, None, None),
        ],
        vec![observed(1, "loot", 3, 10), observed(9, "loot", 5, 25)],
        85,
    );
    let order = |column, descending| {
        let mut rows: Vec<&Drop> = item.of(DropKind::Npc).collect();
        rows.sort_by(|a, b| compare(a, b, column, descending));
        rows.iter().map(|d| d.source_id).collect::<Vec<_>>()
    };
    // Chance: 3 and 9 have none, they stay last both ways (by name: "Nouveau monstre" < "Source 3").
    assert_eq!(order(DropColumn::Chance, true), vec![2, 1, 9, 3]);
    assert_eq!(order(DropColumn::Chance, false), vec![1, 2, 9, 3]);
    // Observed: 9 → 20 %, 1 → 30 %, 2 → 0 / 50; 3 never killed.
    assert_eq!(order(DropColumn::Observed, true), vec![1, 9, 2, 3]);
    assert_eq!(order(DropColumn::Observed, false), vec![2, 9, 1, 3]);
    // Level: 9 has no level (only seen in game).
    assert_eq!(order(DropColumn::Level, false), vec![1, 2, 3, 9]);
    assert_eq!(order(DropColumn::Source, true), vec![3, 2, 1, 9]);
}

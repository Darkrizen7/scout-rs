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
    assert!(npc.is_normal());
    assert_eq!(npc.place(), "Bois de la Pénombre (25.1, 38.2)");
    npc.max_level = Some(24);
    npc.rank = Some(1);
    npc.dungeon = Some("Les Mortemines".into());
    assert_eq!(npc.levels(), "24 élite");
    assert!(!npc.is_normal());
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

#[test]
fn zone_merges_monsters_and_estimates_value() {
    let npc = |npc_id, name: &str, level, kills| ZoneNpcRow {
        npc_id,
        name: Some(name.into()),
        min_level: Some(level),
        max_level: Some(level),
        rank: Some(0),
        zone: Some("Les Tarides".into()),
        x: None,
        y: None,
        dungeon: None,
        ui_map: Some(1413),
        kills: Some(kills),
        skins: Some(0),
    };
    let drop = |npc_id, item_id, chance| ZoneDropRow {
        npc_id,
        kind: "npc".into(),
        item_id,
        chance,
        quest_only: false,
    };
    let zone = ZoneLoot::new(
        "Les Tarides".into(),
        vec![npc(2, "Zhévra", 15, 40), npc(1, "Hyène", 12, 0)],
        vec![
            drop(2, 10, Some(5.0)),
            drop(2, 11, Some(50.0)),
            drop(1, 10, None),
            drop(9, 10, Some(1.0)), // monster of another zone: ignored
        ],
        vec![
            // 40 corpses: the observed 25 % (2 per drop) replaces the reference 5 %.
            ZoneObservedRow {
                npc_id: 2,
                kind: "loot".into(),
                item_id: 10,
                drops: 10,
                quantity: 20,
            },
            ZoneObservedRow {
                npc_id: 2,
                kind: "loot".into(),
                item_id: 12,
                drops: 4,
                quantity: 4,
            },
        ],
    );
    let names: Vec<String> = zone.npcs.iter().map(ZoneNpc::name).collect();
    assert_eq!(names, ["Hyène", "Zhévra"]);
    let zhevra = &zone.npcs[1];
    let items: Vec<i64> = zhevra.drops.iter().map(|d| d.item_id).collect();
    // 11: never seen in 40 corpses, the trusted 0 % beats the reference 50 %.
    assert_eq!(items, [10, 12, 11]);
    assert_eq!(zhevra.drops[2].observed.unwrap().total, 40);
    assert_eq!(zhevra.drops[1].chance, None);
    // 0.25 × 2 × 10 + 0 × 100 + 0.1 × (no price) = 5.
    let price = |id| match id {
        10 => Some(10.0),
        11 => Some(100.0),
        _ => None,
    };
    assert_eq!(zhevra.value_per_kill(price), Some(5.0));
    assert_eq!(zone.npcs[0].value_per_kill(price), None);
}

#[test]
fn sums_up_zones_with_usual_levels() {
    let row = |zone: &str, level, kills| ZoneNpcLevel {
        zone: zone.into(),
        dungeon: false,
        ui_map: Some(1431),
        level: Some(level),
        kills,
    };
    let mut rows: Vec<ZoneNpcLevel> = (20..=30).map(|l| row("Bois", l, None)).collect();
    rows.push(row("Bois", 1, Some(4)));
    rows.push(row("Bois", 62, Some(6)));
    rows.push(row("Azshara", 50, None));
    let zones = zones(rows);
    assert_eq!(zones[0].name, "Azshara");
    let bois = &zones[1];
    assert_eq!((bois.npcs, bois.kills, bois.ui_map), (13, 10, Some(1431)));
    // The level 1 critter and the level 62 guard are left out.
    assert_eq!((bois.min_level, bois.max_level), (Some(20), Some(30)));
}

//! Tests on the real machine, all `#[ignore]`d: `cargo test --workspace -- --ignored --nocapture`.

use std::collections::HashMap;

use anyhow::Result;
use scout_core::GAME_FORMAT;
use scout_core::analysis;
use scout_core::gamedata::GameData;
use scout_core::scout_api::AuctionHouse;
use scout_db::Db;
use scout_sources as sources;
use scout_sources::wow;
use scout_worker::{Config, database_path};

fn throwaway_db(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("scout-rs-{name}-{}.db", std::process::id()))
}

/// Imports the local WoW install into a throwaway database.
#[tokio::test]
#[ignore = "reads the local WoW install"]
async fn imports_local_saved_variables() -> Result<()> {
    let config = Config::load();
    let wow_dir = config.wow_dir()?;
    let saved_variables = wow::saved_variables(wow_dir)?;
    let scout = sources::run_scout_export(&wow::export_script(wow_dir)?, &saved_variables)?;
    let loot = sources::run_scout_loot(&wow::export_script(wow_dir)?, &saved_variables)?;

    let path = throwaway_db("local");
    let db = Db::open(&path).await?;
    let added = db.insert(&scout).await?;
    assert_eq!(db.insert(&scout).await?, 0, "a second import adds nothing");
    let kills = db.save_observed_loot(&loot).await?;
    let items = db.items(&AuctionHouse::default()).await?;
    println!(
        "{} scout rows, {added} stored, {} items, {kills} kills",
        scout.len(),
        items.len()
    );
    for id in [2589, 8191] {
        for s in db.history(id, &AuctionHouse::default()).await? {
            println!("{id} {} {} {:?} {:?}", s.source, s.ts, s.price, s.quantity);
        }
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

/// Full download from wago.tools into a throwaway database (about 12 MB).
#[tokio::test]
#[ignore = "downloads about 12 MB from wago.tools"]
async fn downloads_assembles_and_stores_latest_build() -> Result<()> {
    let build = scout_gamedata::latest_build()?;
    let mut tables = HashMap::new();
    for table in scout_gamedata::TABLES {
        tables.insert(table, scout_gamedata::table_csv(table, &build)?);
    }
    let tables = scout_gamedata::assemble(&tables)?;
    let missing = tables.missing_recipe_items();
    let categorized = missing
        .iter()
        .filter(|id| tables.unnamed_categories.contains_key(id))
        .count();
    println!(
        "{} objets de recettes absents des tables (à demander à Wowhead), {categorized} avec catégorie",
        missing.len()
    );
    let game = GameData::new(build.clone(), 1, tables);

    let path = throwaway_db("game");
    let db = Db::open(&path).await?;
    db.save_game(&game).await?;
    let loaded = db.load_game().await?;
    std::fs::remove_file(&path)?;

    assert_eq!(
        db.game_version().await?,
        Some((build.clone(), GAME_FORMAT, game.updated))
    );
    assert_eq!(
        (loaded.recipes.len(), loaded.items.len()),
        (game.recipes.len(), game.items.len())
    );
    assert_eq!(loaded.maps, game.maps);
    for continent in [1415, 1414] {
        let zones = scout_core::map::zones_of(&loaded.maps, continent);
        let names: Vec<&str> = zones.iter().map(|m| m.name.as_str()).collect();
        println!(
            "carte {continent} : {} zones : {}",
            zones.len(),
            names.join(", ")
        );
    }
    println!(
        "Forever {build}: {} recettes, {} objets, {} métiers",
        loaded.recipes.len(),
        loaded.items.len(),
        loaded.professions.len()
    );
    for p in &loaded.professions {
        println!(
            "  {} : {}",
            p.name,
            loaded
                .recipes
                .iter()
                .filter(|r| r.profession == p.id)
                .count()
        );
    }
    let linen = 2589;
    println!(
        "{} : utilisé dans {} recettes",
        loaded.item_name(linen),
        loaded.used_in.get(&linen).map_or(0, Vec::len)
    );
    let boots = loaded
        .recipes
        .iter()
        .find(|r| r.spell_id == 1301456)
        .expect("recette Forever");
    println!(
        "{} : requis {:?}, composants {:?}, patron {:?}",
        boots.name, boots.required, boots.reagents, boots.taught_by
    );
    Ok(())
}

/// Runs the analysis on the real database.
#[tokio::test]
#[ignore = "reads the local database"]
async fn summarizes_local_database() -> Result<()> {
    let db = Db::open(&database_path()?).await?;
    let start = std::time::Instant::now();
    let house = scout_worker::Config::load().price_server.auction_house();
    let a = analysis::analyze(&db.observations(&house).await?);
    println!("analysed in {:?}", start.elapsed());
    for d in &a.days {
        println!(
            "{} weekend={} index={:+.1}% items={}",
            d.date,
            d.weekend,
            (d.index - 1.0) * 100.0,
            d.items
        );
    }
    println!("market weekend: {:?}", a.market_weekend);
    let weekend: Vec<_> = a.items.iter().filter_map(|i| i.weekend.as_ref()).collect();
    let today: Vec<_> = a.items.iter().filter_map(|i| i.latest.as_ref()).collect();
    println!(
        "items {} · weekend tested {} (significant {}) · today tested {} (significant {})",
        a.items.len(),
        weekend.len(),
        weekend.iter().filter(|w| w.significant()).count(),
        today.len(),
        today.iter().filter(|o| o.significant()).count()
    );
    let mut flagged: Vec<_> = a
        .items
        .iter()
        .filter(|i| i.latest.as_ref().is_some_and(|o| o.significant()))
        .collect();
    flagged.sort_by_key(|i| i.item_id);
    for i in flagged.iter().step_by(flagged.len().max(1) / 12 + 1) {
        let o = i.latest.as_ref().unwrap();
        println!(
            "{} q={:?} {} price={} base={} {:+.0}% z={:.1}",
            i.item_id, i.median_quantity, o.date, o.price, o.baseline, o.diff_pct, o.z
        );
    }
    Ok(())
}

/// Loot by zone on a copy of the local database: zone list, then the best monsters of one zone.
#[tokio::test]
#[ignore = "reads the local database"]
async fn lists_zone_loot() -> Result<()> {
    let path = throwaway_db("zones");
    std::fs::copy(database_path()?, &path)?;
    let db = Db::open(&path).await?;
    let started = std::time::Instant::now();
    let zones = db.zones().await?;
    println!("{} zones in {:?}", zones.len(), started.elapsed());
    for z in zones.iter().take(5) {
        println!("  {z:?}");
    }
    let started = std::time::Instant::now();
    let loot = db.zone_loot("Les Tarides").await?;
    println!(
        "{}: {} monsters in {:?}",
        loot.zone,
        loot.npcs.len(),
        started.elapsed()
    );
    for npc in loot.npcs.iter().take(3) {
        println!(
            "  {} {} : {} drops",
            npc.name(),
            npc.npc.levels(),
            npc.drops.len()
        );
        for d in npc.drops.iter().take(4) {
            println!("    {:?} {} {:?}", d.kind, d.item_id, d.chance);
        }
    }
    assert!(loot.npcs.len() > 100);
    std::fs::remove_file(&path)?;
    Ok(())
}

/// OpenMap's zone shapes from the local install.
#[test]
#[ignore = "reads the local WoW install"]
fn reads_open_map_masks() -> Result<()> {
    let config = Config::load();
    let dir = wow::open_map_masks(config.wow_dir()?).expect("OpenMap installé");
    let started = std::time::Instant::now();
    let masks = sources::zone_masks(&dir)?;
    println!("{} masks in {:?}", masks.len(), started.elapsed());
    let duskwood = &masks[&1431];
    println!(
        "1431: {}x{}, center {:?}",
        duskwood.width,
        duskwood.height,
        duskwood.center()
    );
    assert!(masks.len() > 40);
    Ok(())
}

/// Items of the Razorbramble leather recipe, missing from the client tables, named by Wowhead.
#[test]
#[ignore = "asks Wowhead for three items"]
fn wowhead_names_items_missing_from_the_tables() -> Result<()> {
    for (id, name) in [
        (233813, "Armure en cuir de Roncerasoir"),
        (234007, "Chitine épineuse de silithide"),
        (234009, "Rouleau de soie qiraji"),
    ] {
        let start = std::time::Instant::now();
        let item = scout_gamedata::wowhead::item(id)?.expect("known by Wowhead");
        println!("{id} {item:?} en {:?}", start.elapsed());
        assert_eq!(item.name, name);
    }
    assert!(scout_gamedata::wowhead::item(999_999)?.is_none());
    Ok(())
}

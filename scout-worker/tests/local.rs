//! Tests on the real machine, all `#[ignore]`d: `cargo test --workspace -- --ignored --nocapture`.

use std::collections::HashMap;

use anyhow::Result;
use scout_core::GAME_FORMAT;
use scout_core::analysis;
use scout_core::gamedata::GameData;
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
    let legacy = sources::run_legacy_export(saved_variables.parent().unwrap())?;

    let path = throwaway_db("local");
    let db = Db::open(&path).await?;
    let added = db.insert(&scout).await? + db.insert(&legacy).await?;
    assert_eq!(db.insert(&legacy).await?, 0, "a second import adds nothing");
    let items = db.items().await?;
    println!(
        "{} scout + {} legacy rows, {added} stored, {} items",
        scout.len(),
        legacy.len(),
        items.len()
    );
    for id in [2589, 8191] {
        for s in db.history(id).await? {
            println!("{id} {} {} {:?} {:?}", s.source, s.ts, s.price, s.quantity);
        }
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

/// Forever Guide's drops and Scout's recorded loot into a throwaway database, then a few items' sources.
#[tokio::test]
#[ignore = "reads the local WoW install"]
async fn imports_drops() -> Result<()> {
    let config = Config::load();
    let wow_dir = config.wow_dir()?;
    let guide = wow::forever_guide(wow_dir).expect("Forever Guide installé");
    let started = std::time::Instant::now();
    let export = sources::run_drops_export(&guide)?;
    let exported = started.elapsed();

    let path = throwaway_db("drops");
    let db = Db::open(&path).await?;
    let fingerprint = wow::guide_fingerprint(&guide, sources::DROPS_FORMAT);
    let count = db.save_drops(&export, &fingerprint).await?;
    assert_eq!(db.drops_fingerprint().await?, Some(fingerprint));
    let saved_variables = wow::saved_variables(wow_dir)?;
    let loot = sources::run_scout_loot(&wow::export_script(wow_dir)?, &saved_variables)?;
    let kills = db.save_observed_loot(&loot).await?;
    println!(
        "{count} sources, {} PNJ, {} noms (export {exported:.1?}, total {:.1?}) ; Scout : {kills} kills",
        export.npcs.len(),
        export.names.len(),
        started.elapsed()
    );
    // Linen cloth, a Sunken Temple boss item, a quest reward, a pattern found in chests.
    for id in [2589, 10799, 1431, 2318] {
        let drops = db.item_drops(id).await?;
        println!(
            "{id} : {} sources, plus {:?}",
            drops.drops.len(),
            drops.more
        );
        for d in drops.drops.iter().take(4) {
            let (levels, place) = d
                .npc
                .as_ref()
                .map(|n| (n.levels(), n.place()))
                .unwrap_or_default();
            println!(
                "  {:?} {} [{levels}] {place} {:?} {:?}",
                d.kind, d.name, d.chance, d.observed
            );
        }
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

/// Full download from wago.tools into a throwaway database (about 12 MB).
#[tokio::test]
#[ignore = "downloads about 12 MB from wago.tools"]
async fn downloads_assembles_and_stores_latest_build() -> Result<()> {
    let build = sources::latest_build()?;
    let mut tables = HashMap::new();
    for table in sources::TABLES {
        tables.insert(table, sources::table_csv(table, &build)?);
    }
    let game = GameData::new(build.clone(), 1, sources::assemble(&tables)?);

    let path = throwaway_db("game");
    let db = Db::open(&path).await?;
    db.save_game(&game).await?;
    let loaded = db.load_game().await?;
    std::fs::remove_file(&path)?;

    assert_eq!(db.game_version().await?, Some((build.clone(), GAME_FORMAT)));
    assert_eq!(
        (loaded.recipes.len(), loaded.items.len()),
        (game.recipes.len(), game.items.len())
    );
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
    let a = analysis::analyze(&db.observations().await?);
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

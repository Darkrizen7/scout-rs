use std::path::{Path, PathBuf};

use anyhow::Result;
use scout_core::GAME_FORMAT;

use super::super::Db;

fn temp_dir(name: &str) -> Result<PathBuf> {
    let dir =
        std::env::temp_dir().join(format!("scout-rs-reference-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A filled database: game data and loot of `updated`, a merchant, and personal data.
async fn filled(path: &Path, updated: i64, item: &str) -> Result<Db> {
    let db = Db::open(path).await?;
    sqlx::query(
        "INSERT INTO game_meta (key, value) VALUES ('build', '1.60'), ('format', ?), ('updated', ?)",
    )
    .bind(GAME_FORMAT.to_string())
    .bind(updated.to_string())
    .execute(&db.pool)
    .await?;
    sqlx::query(
        "INSERT INTO game_items (item_id, name, quality, item_level, required_level) VALUES (1, ?, 1, 5, 0)",
    )
    .bind(item)
    .execute(&db.pool)
    .await?;
    sqlx::raw_sql(
        "INSERT INTO drop_sources (item_id, kind, source_id, chance) VALUES (1, 'npc', 3100, 12.5);
         INSERT INTO drop_npcs (npc_id, name, zone) VALUES (3100, 'Ancien sanglier tacheté', 'Durotar');
         INSERT INTO vendor_prices (item_id, source, npc_id, price) VALUES (1, 'scout', 7, 40);
         INSERT INTO items (item_id, name) VALUES (1, 'Lin');
         INSERT INTO snapshots (item_id, source, ts, price) VALUES (1, 'scout', 100, 50);
         INSERT INTO favorites (item_id) VALUES (1);
         INSERT INTO loot_npcs (npc_id, kills, skins) VALUES (3100, 10, 11);",
    )
    .execute(&db.pool)
    .await?;
    sqlx::query("INSERT INTO drop_meta (key, value) VALUES ('updated', ?)")
        .bind(updated.to_string())
        .execute(&db.pool)
        .await?;
    Ok(db)
}

async fn count(db: &Db, sql: &'static str) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as(sql).fetch_one(&db.pool).await?;
    Ok(n)
}

#[tokio::test]
async fn applies_server_package_keeping_local_merchants() -> Result<()> {
    let dir = temp_dir("server")?;
    let source = filled(&dir.join("source.db"), 200, "Lin").await?;
    // The package's merchant is another one, plus a competing row for the merchant Scout saw.
    sqlx::raw_sql(
        "DELETE FROM drop_sources;
         INSERT INTO drop_sources (item_id, kind, source_id, chance) VALUES (2, 'npc', 5, 1.0);
         DELETE FROM vendor_prices;
         INSERT INTO vendor_prices (item_id, source, npc_id, price) VALUES (1, 'scout', 7, 99), (3, 'guide', 8, 10);",
    )
    .execute(&source.pool)
    .await?;
    source.close().await;
    let package = dir.join("package.db");
    Db::export_loot_vendors(&dir.join("source.db"), &package).await?;

    let local = filled(&dir.join("local.db"), 100, "Lin").await?;
    assert_eq!(local.server_reference().await?, None);
    local.apply_loot_vendors(&package).await?;
    assert_eq!(
        count(
            &local,
            "SELECT COUNT(*) FROM drop_sources WHERE item_id = 2"
        )
        .await?,
        1
    );
    assert_eq!(count(&local, "SELECT COUNT(*) FROM drop_sources").await?, 1);
    assert_eq!(
        count(
            &local,
            "SELECT CAST(price AS INTEGER) FROM vendor_prices WHERE item_id = 1"
        )
        .await?,
        40
    );
    assert_eq!(
        count(&local, "SELECT COUNT(*) FROM vendor_prices").await?,
        2
    );
    // Personal data untouched.
    assert_eq!(count(&local, "SELECT COUNT(*) FROM favorites").await?, 1);

    local.set_server_reference(1, 1234).await?;
    assert_eq!(local.server_reference().await?, Some((1, 1234)));

    // A package without loot is refused and the local loot stays.
    let empty = Db::open(&dir.join("empty.db")).await?;
    empty.close().await;
    assert!(
        local
            .apply_loot_vendors(&dir.join("empty.db"))
            .await
            .is_err()
    );
    assert_eq!(count(&local, "SELECT COUNT(*) FROM drop_sources").await?, 1);

    local.close().await;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[tokio::test]
async fn item_names_fill_gaps_without_overwriting() -> Result<()> {
    let dir = temp_dir("items")?;
    let source = filled(&dir.join("source.db"), 200, "Lin").await?;
    sqlx::raw_sql(
        "DELETE FROM snapshots; DELETE FROM items;
         INSERT INTO items (item_id, name, class_id, subclass_id, class_name, subclass_name) VALUES
            (1, 'Nom serveur', 7, 5, 'Artisanat', 'Tissu'),
            (2, 'Manquant', 7, 5, 'Artisanat', 'Tissu'),
            (3, 'Ajouté', 2, 1, 'Arme', 'Hache'),
            (4, '', NULL, NULL, NULL, NULL);",
    )
    .execute(&source.pool)
    .await?;
    source.close().await;
    let package = dir.join("package.db");
    Db::export_loot_vendors(&dir.join("source.db"), &package).await?;

    let local = filled(&dir.join("local.db"), 100, "Lin").await?;
    sqlx::raw_sql(
        "DELETE FROM snapshots; DELETE FROM items;
         INSERT INTO items (item_id, name, class_id) VALUES (1, 'Nom local', 9), (2, '', NULL);",
    )
    .execute(&local.pool)
    .await?;
    local.apply_loot_vendors(&package).await?;
    let rows: Vec<(i64, String, Option<i64>, Option<String>)> =
        sqlx::query_as("SELECT item_id, name, class_id, class_name FROM items ORDER BY item_id")
            .fetch_all(&local.pool)
            .await?;
    let cat = |s: &str| Some(s.to_string());
    assert_eq!(
        rows,
        vec![
            (1, "Nom local".into(), Some(9), cat("Artisanat")),
            (2, "Manquant".into(), Some(7), cat("Artisanat")),
            (3, "Ajouté".into(), Some(2), cat("Arme")),
        ]
    );
    // Nothing personal in the package.
    let package_db = Db::open(&package).await?;
    assert_eq!(count(&package_db, "SELECT COUNT(*) FROM items").await?, 3);
    package_db.close().await;
    local.close().await;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Simulation on real copies: `SIM_DB` (local) and `SIM_PACKAGE` (decompressed package), houses eu 111 / 114.
#[tokio::test]
#[ignore = "needs SIM_DB and SIM_PACKAGE"]
async fn simulate_item_names() -> Result<()> {
    use scout_core::scout_api::AuctionHouse;
    let db = Db::open(Path::new(&std::env::var("SIM_DB")?)).await?;
    let unnamed = async |db: &Db, id: u32| -> Result<usize> {
        let house = AuctionHouse {
            region: "eu".into(),
            id,
        };
        Ok(db
            .items(&house)
            .await?
            .iter()
            .filter(|i| i.name.as_deref().is_none_or(str::is_empty))
            .count())
    };
    for id in [111, 114] {
        println!("avant {id} : {}", unnamed(&db, id).await?);
    }
    db.apply_loot_vendors(Path::new(&std::env::var("SIM_PACKAGE")?))
        .await?;
    for id in [111, 114] {
        println!("après {id} : {}", unnamed(&db, id).await?);
    }
    db.close().await;
    Ok(())
}

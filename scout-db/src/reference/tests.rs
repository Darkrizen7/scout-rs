use std::path::{Path, PathBuf};

use anyhow::Result;
use scout_core::GAME_FORMAT;

use super::super::Db;
use super::ReferenceUpdate;

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

async fn item_name(db: &Db) -> Result<String> {
    let (name,): (String,) = sqlx::query_as("SELECT name FROM game_items WHERE item_id = 1")
        .fetch_one(&db.pool)
        .await?;
    Ok(name)
}

#[tokio::test]
async fn exports_reference_data_only_and_applies_what_is_newer() -> Result<()> {
    let dir = temp_dir("export")?;
    let source = filled(&dir.join("source.db"), 200, "Lin").await?;
    source.close().await;
    let seed = dir.join("reference.db");
    Db::export_reference(&dir.join("source.db"), &seed).await?;

    // A new database gets everything but the personal data.
    let fresh = Db::open(&dir.join("fresh.db")).await?;
    let all = ReferenceUpdate {
        game: true,
        drops: true,
    };
    assert_eq!(fresh.apply_reference(&seed).await?, all);
    assert_eq!(item_name(&fresh).await?, "Lin");
    assert_eq!(count(&fresh, "SELECT COUNT(*) FROM drop_npcs").await?, 1);
    assert_eq!(
        count(&fresh, "SELECT COUNT(*) FROM vendor_prices").await?,
        1
    );
    for personal in [
        "SELECT COUNT(*) FROM items",
        "SELECT COUNT(*) FROM snapshots",
        "SELECT COUNT(*) FROM favorites",
        "SELECT COUNT(*) FROM loot_npcs",
    ] {
        assert_eq!(count(&fresh, personal).await?, 0, "{personal}");
    }
    // Applied again: nothing newer, merchants not doubled.
    assert_eq!(
        fresh.apply_reference(&seed).await?,
        ReferenceUpdate::default()
    );
    assert_eq!(
        count(&fresh, "SELECT COUNT(*) FROM vendor_prices").await?,
        1
    );

    // Newer local data (a later download) stays.
    let newer = filled(&dir.join("newer.db"), 300, "Tissu de lin").await?;
    assert_eq!(
        newer.apply_reference(&seed).await?,
        ReferenceUpdate::default()
    );
    assert_eq!(item_name(&newer).await?, "Tissu de lin");

    // Loot imported before its date was kept is older than the reference.
    sqlx::query("DELETE FROM drop_meta")
        .execute(&newer.pool)
        .await?;
    assert!(newer.apply_reference(&seed).await?.drops);

    fresh.close().await;
    newer.close().await;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
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
